// EN: Real `WireCommitExecutor` for Wire 1:1 multi-leaf commits (CHAT_MULTIDEVICE_HYBRID_DESIGN
// §4.3, CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC §4). The coordinator-device (CD) lifecycle driver
// delegates the OpenMLS work here. It runs the op as a STAGED commit (no merge), so a lost
// `(conv, epoch)` CAS race can be cleanly discarded (`commitAbandoned` / `catchUpAndRerun`) instead
// of force-merging a forked epoch — 1:1 has no on-chain commit log to recover from. The relay's CAS
// verdict drives the merge: `commitAccepted` (merge) on ACCEPT, clear on EPOCH_STALE/give-up.
// Backed by a tiny engine subset (satisfied by `OpenMlsEngine`) so it is unit-testable against the
// real WASM engine.
//
// CN: Wire 1:1 多 leaf commit 的真实 `WireCommitExecutor`（设计 §4.3、串行化规范 §4）。协调设备（CD）
// 生命周期驱动把 OpenMLS 工作委托到此。操作跑成 STAGED commit（不合并），使落败的 `(conv, epoch)` CAS
// 竞争可被干净丢弃（`commitAbandoned` / `catchUpAndRerun`），而非强制合并出分叉 epoch——1:1 无链上 commit
// 日志可恢复。relay 的 CAS 裁决驱动合并：ACCEPT 用 `commitAccepted`（合并），EPOCH_STALE/放弃则清除。
// 依赖极小引擎子集（`OpenMlsEngine` 满足），可对真实 WASM 引擎单测。

import { directMlsKeyInvolves } from "@/mls/directConv";
import type { CommitIntentControlMsg } from "@/mls/directCommitCoordination";
import type { WireCommitExecutor } from "@/mls/directAccountCommitCoordinator";
import { b64ToBytes, bytesToB64, type RelayClient } from "@/relay/relayClient";

/// EN: Minimal staged-commit OpenMLS surface the executor needs (a subset of `OpenMlsEngine`).
/// CN: 执行器所需的最小 staged-commit OpenMLS 接口（`OpenMlsEngine` 的子集）。
export interface WireExecutorEngine {
  hasGroup(convId: string): boolean;
  epochByConv(convId: string): number;
  addMembersStagedByConv(
    convId: string,
    keyPackages: Uint8Array[],
  ): { commit: Uint8Array; welcome: Uint8Array };
  removeMembersStagedByConv(
    convId: string,
    memberIdentities: string[],
  ): { commit: Uint8Array; welcome: Uint8Array };
  selfUpdateStagedByConv(convId: string): Uint8Array;
  mergePendingByConv(convId: string): void;
  clearPendingByConv(convId: string): void;
}

export interface AddDeviceExecutorDeps {
  engine: WireExecutorEngine;
  relay: Pick<RelayClient, "sendControl" | "requestMlsBacklog">;
  endpointId: string;
  selfAddress: string;
}

/// EN: Build a `WireCommitExecutor` for `add_device` / `rekey` / `remove_device` intents.
/// `remove_device` targets a single leaf via the device-distinct `{account}#{deviceId}` credential
/// (HYBRID_DESIGN §4.2) carried in `payload.target`; it ONLY works when the 1:1 engine uses
/// device-distinct leaf identities (see `deviceLeafIdentity`). CN: 为 `add_device` / `rekey` /
/// `remove_device` 意图构造 `WireCommitExecutor`。`remove_device` 经 `payload.target` 中的设备区分
/// `{account}#{deviceId}` 凭证（设计 §4.2）定位单个 leaf；仅当 1:1 引擎使用设备区分 leaf identity 时有效
/// （见 `deviceLeafIdentity`）。
export function createAddDeviceExecutor(deps: AddDeviceExecutorDeps): WireCommitExecutor {
  function assertConv(intent: CommitIntentControlMsg): string {
    const convId = intent.payload.dmConvId;
    if (!directMlsKeyInvolves(convId, deps.selfAddress)) {
      throw new Error(`WireCommitExecutor: ${deps.selfAddress} is not a party to ${convId}`);
    }
    if (!deps.engine.hasGroup(convId)) {
      throw new Error(`WireCommitExecutor: no local 1:1 group for ${convId}`);
    }
    return convId;
  }

  function addDeviceKp(intent: CommitIntentControlMsg): Uint8Array {
    if (!intent.payload.kp) {
      throw new Error("WireCommitExecutor: add_device intent missing KeyPackage");
    }
    return b64ToBytes(intent.payload.kp);
  }

  // EN: Stage the op for an intent at the current local epoch (no merge). CN: 在当前本地 epoch 把
  // 意图的操作暂存（不合并）。
  function stage(intent: CommitIntentControlMsg, convId: string): {
    commitB64: string;
    welcomeB64: string;
  } {
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
        throw new Error("WireCommitExecutor: remove_device intent missing target device identity");
      }
      const out = deps.engine.removeMembersStagedByConv(convId, [target]);
      // EN: a removal MAY carry a Welcome (re-add of remaining members' secrets); forward if present.
      // CN: 移除**可能**带 Welcome（为余下成员重投密钥）；存在则转发。
      return { commitB64: bytesToB64(out.commit), welcomeB64: bytesToB64(out.welcome) };
    }
    throw new Error(`WireCommitExecutor: unsupported intent kind ${intent.kind}`);
  }

  return {
    async runIntent(intent) {
      const convId = assertConv(intent);
      // EN: capture epoch BEFORE staging — staging does NOT advance the local epoch, so this is the
      // wire `commit_epoch` (pre-op = current). CN: 在暂存**前**捕获 epoch——暂存不推进本地 epoch，故
      // 此即 wire `commit_epoch`（操作前 = 当前）。
      const preEpoch = deps.engine.epochByConv(convId);
      const out = stage(intent, convId);
      return { ...out, preEpoch };
    },

    async commitAccepted(intent) {
      deps.engine.mergePendingByConv(intent.payload.dmConvId);
    },

    async commitAbandoned(intent) {
      // EN: clearing a non-pending group is a safe no-op in OpenMLS. CN: 对无 pending 的群清除是安全空操作。
      deps.engine.clearPendingByConv(intent.payload.dmConvId);
    },

    async catchUpAndRerun(intent, toEpoch) {
      const convId = assertConv(intent);
      // EN: discard our forked staged commit FIRST — always safe, returns the group to operational
      // at its pre-op epoch and prevents a permanent 1:1 fork. CN: **先**丢弃我方分叉暂存 commit——
      // 永远安全，使群回到操作前 epoch 的 operational 态，防 1:1 永久分叉。
      deps.engine.clearPendingByConv(convId);
      const epoch = deps.engine.epochByConv(convId);
      if (epoch < toEpoch) {
        // EN: the winning commit has not yet been applied to our local group. We fail fast so the
        // lifecycle re-polls; `requestCatchUp` (below) actively pulls the winner from the relay so a
        // subsequent poll succeeds deterministically. Budget exhaustion → give-up → recover/re-
        // handshake (spec §5). CN: 胜出 commit 尚未应用到本地群。快速失败让生命周期重轮询；下方
        // `requestCatchUp` 主动从 relay 拉取胜出 commit，使后续轮询确定性成功。预算耗尽 → give-up →
        // recover/重握手（规范 §5）。
        throw new Error(
          `WireCommitExecutor: local 1:1 group at epoch ${epoch} < ${toEpoch}; awaiting winning commit`,
        );
      }
      const out = stage(intent, convId);
      return { ...out, newEpoch: epoch };
    },

    requestCatchUp(convId) {
      // EN: ask the relay to re-deliver this conv's stored winning Commit(s) to our account so the
      // registry applies it and the next catch-up poll succeeds (spec §3.3). CN: 请求 relay 把该会话
      // 已存胜出 Commit 重投到本账户，使 registry 应用之、下次追平轮询成功（规范 §3.3）。
      deps.relay.requestMlsBacklog?.(deps.selfAddress, convId);
    },

    async deliverWelcome(intent, welcomeB64) {
      // EN: rekey / member-less ops carry no Welcome. CN: rekey / 无新成员操作无 Welcome。
      if (!welcomeB64) return;
      // EN: The joining device's ACCOUNT receives the Welcome (relay fans it to all that account's
      // devices; existing members ignore it). For a sibling add that's my own account; for a
      // PEER-ASSISTED add (§3.8) it's the requester's account, carried in `payload.welcomeTo`.
      // CN: 加入设备的**账户**收 Welcome（relay 扇出到该账户所有设备；已是成员的忽略之）。兄弟 add 即
      // 我自己的账户；**对端代 Add**（§3.8）则是请求方账户，置于 `payload.welcomeTo`。
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
