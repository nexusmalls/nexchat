// EN: Gate-1 CD-side Commit lifecycle reducer for Wire 1:1 (CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_
// SPEC §4.1/§3.3). PURE state machine — no relay IO, no OpenMLS. The coordinator device (CD), after
// running an OpenMLS add/remove/rekey (which auto-merges locally, so the wire `commit_epoch` is the
// epoch captured BEFORE the op), must serialize the Commit through the relay's `(conv, epoch)` CAS:
//   send Commit → (implicit accept: no reject within the window) → send Welcome + reply ok;
//   or `commit_reject{epoch_stale}` → catch up to the relay epoch and retry, bounded by
//   MAX_COMMIT_RETRY, else reply fail. Acceptance is implicit because the relay only answers on
//   rejection (§3.2): a settle timeout with no reject means the Commit won the epoch.
// This reducer maps (state, event) → (next state, side-effect actions) so the async driver
// (directAccountCommitCoordinator) stays a thin shell over deterministic, unit-tested logic.
//
// CN: Wire 1:1 的 CD 侧 Commit 生命周期 reducer（规范 §4.1/§3.3）。**纯**状态机——无 relay IO、无 OpenMLS。
// 协调设备（CD）跑完 OpenMLS add/remove/rekey 后（本地自动合并，故 wire `commit_epoch` 取操作**之前**捕获的
// epoch），须经 relay 的 `(conv, epoch)` CAS 串行化 Commit：发 Commit →（隐式采纳：窗口内无 reject）→ 发
// Welcome + 回 ok；或 `commit_reject{epoch_stale}` → 追平到 relay epoch 后重试，受 MAX_COMMIT_RETRY 限制，
// 超限则回 fail。采纳是隐式的，因为 relay 仅在拒绝时应答（§3.2）：静默超时无 reject 即表示该 Commit 赢得 epoch。
// reducer 把 (state, event) → (下一状态, 副作用动作)，使异步驱动（directAccountCommitCoordinator）成为
// 确定性、单测覆盖逻辑之上的薄壳。

import {
  MAX_COMMIT_RETRY,
  type CommitIntentKind,
  type CommitIntentPayload,
} from "@/mls/directCommitCoordination";

/// EN: One in-flight Commit attempt the CD is driving. CN: CD 正在驱动的一次 Commit 尝试。
export interface CommitAttempt {
  /** EN: Target pairwise `d:` conv. CN: 目标 pairwise `d:` conv。 */
  convId: string;
  /** EN: Originating intent (for replay / result correlation). CN: 触发意图（用于重放 / 结果关联）。 */
  reqId: string;
  kind: CommitIntentKind;
  payload: CommitIntentPayload;
}

export type CommitPhase = "awaiting" | "delivered" | "failed";

export interface CommitLifecycleState {
  attempt: CommitAttempt;
  phase: CommitPhase;
  /** EN: `commit_epoch` of the in-flight Commit (pre-op epoch). CN: 在途 Commit 的 `commit_epoch`（操作前 epoch）。 */
  commitEpoch: number;
  /** EN: Idempotency / correlation key of the in-flight Commit. CN: 在途 Commit 的幂等 / 关联键。 */
  msgId: string;
  /** EN: Number of `epoch_stale` retries already consumed. CN: 已消耗的 `epoch_stale` 重试次数。 */
  retries: number;
}

export type CommitLifecycleEvent =
  /** EN: Settle window elapsed with NO reject → relay accepted (§3.2). CN: 静默窗口结束且无 reject → relay 采纳（§3.2）。 */
  | { t: "settle_timeout"; msgId: string }
  /** EN: Relay said we lost the `(conv, epoch)` race. CN: relay 告知我们在 `(conv, epoch)` 竞争中落败。 */
  | { t: "epoch_stale"; msgId: string; currentEpoch: number }
  /** EN: Local catch-up to `newEpoch` finished; ready to re-commit. CN: 本地追平到 `newEpoch` 完成；可重发 Commit。 */
  | { t: "caught_up"; newEpoch: number; commitB64: string; welcomeB64: string; newMsgId: string };

/// EN: A side-effect the driver must perform. CN: 驱动需执行的副作用。
export type CommitAction =
  /** EN: Deliver Welcome to the joining device, then reply `commit_result{ok}` over `s:<account>`. CN: 向加入设备投递 Welcome，再经 `s:<account>` 回 `commit_result{ok}`。 */
  | { t: "deliver_welcome_and_ok" }
  /** EN: Catch up local group to `currentEpoch`, then re-run the MLS op (driver emits `caught_up`). CN: 本地追平到 `currentEpoch`，再重跑 MLS 操作（驱动随后发 `caught_up`）。 */
  | { t: "catch_up_and_retry"; currentEpoch: number }
  /** EN: Re-send the Commit with the new epoch + msgId. CN: 用新 epoch + msgId 重发 Commit。 */
  | { t: "resend_commit"; commitEpoch: number; commitB64: string; welcomeB64: string; msgId: string }
  /** EN: Reply `commit_result{ok:false, epoch_stale}` — gave up after the retry budget. CN: 回 `commit_result{ok:false, epoch_stale}`——超重试预算放弃。 */
  | { t: "reply_give_up"; currentEpoch: number };

export interface CommitLifecycleStep {
  state: CommitLifecycleState;
  actions: CommitAction[];
}

/// EN: Open a new in-flight Commit (after the CD ran the MLS op and captured the pre-op epoch).
/// CN: 开一次新的在途 Commit（CD 跑完 MLS 操作并捕获操作前 epoch 后）。
export function startCommit(args: {
  attempt: CommitAttempt;
  commitEpoch: number;
  msgId: string;
}): CommitLifecycleState {
  return {
    attempt: args.attempt,
    phase: "awaiting",
    commitEpoch: args.commitEpoch,
    msgId: args.msgId,
    retries: 0,
  };
}

/// EN: Pure transition. Stale events for a non-current `msgId` are ignored (late duplicates).
/// CN: 纯转移。非当前 `msgId` 的过期事件被忽略（迟到的重复）。
export function reduceCommit(
  state: CommitLifecycleState,
  event: CommitLifecycleEvent,
): CommitLifecycleStep {
  if (state.phase !== "awaiting") {
    return { state, actions: [] };
  }
  switch (event.t) {
    case "settle_timeout": {
      if (event.msgId !== state.msgId) return { state, actions: [] };
      return {
        state: { ...state, phase: "delivered" },
        actions: [{ t: "deliver_welcome_and_ok" }],
      };
    }
    case "epoch_stale": {
      if (event.msgId !== state.msgId) return { state, actions: [] };
      if (state.retries >= MAX_COMMIT_RETRY) {
        return {
          state: { ...state, phase: "failed" },
          actions: [{ t: "reply_give_up", currentEpoch: event.currentEpoch }],
        };
      }
      return {
        state,
        actions: [{ t: "catch_up_and_retry", currentEpoch: event.currentEpoch }],
      };
    }
    case "caught_up": {
      return {
        state: {
          ...state,
          commitEpoch: event.newEpoch,
          msgId: event.newMsgId,
          retries: state.retries + 1,
        },
        actions: [
          {
            t: "resend_commit",
            commitEpoch: event.newEpoch,
            commitB64: event.commitB64,
            welcomeB64: event.welcomeB64,
            msgId: event.newMsgId,
          },
        ],
      };
    }
  }
}
