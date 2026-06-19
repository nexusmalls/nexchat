// EN: Group commit ordering driver (CHAT_GROUP_WIREIFY_DESIGN §7 / §15.3 / G3). Drives a single group
// device-op (`add_device` / `remove_device` / `rekey`) to chain-serialized completion. Unlike 1:1 Wire
// — which serializes via the relay's off-chain `commit_slot` CAS with an implicit-accept settle window
// — a GROUP serializes via the chain `commit` extrinsic, whose `expected_epoch` check IS the atomic
// compare-and-swap (block total order). So the loop is simpler and has no settle timer:
//   1. `executor.runIntent` → STAGED commit + pre-op epoch (the `expected_epoch`).
//   2. submit the chain `commit` at `expected_epoch` (the CAS).
//   3. Ok            → `commitAccepted` (merge) + `deliverWelcome` over `s:<account>`.
//      EpochStale    → `catchUpAndRerun` (pull winning chain commits, re-stage) and retry, bounded.
//      other error   → `commitAbandoned` (discard the staged fork) and surface.
// The driver is decoupled from the live chain via `submitGroupCommit`, so it is unit-testable against a
// fake CAS and the real OpenMLS executor.
//
// CN: 群 commit 定序驱动（设计 §7 / §15.3 / G3）。把单个群设备操作（`add_device` / `remove_device` /
// `rekey`）驱动到链定序完成。与 1:1 Wire（经 relay 链下 `commit_slot` CAS + 隐式采纳静默窗口定序）不同，
// **群**经链上 `commit` extrinsic 定序，其 `expected_epoch` 检查**即**原子 CAS（区块全序）。故循环更简单、无
// 静默计时器：
//   1. `executor.runIntent` → STAGED commit + 操作前 epoch（即 `expected_epoch`）。
//   2. 以 `expected_epoch` 提交链上 `commit`（CAS）。
//   3. Ok            → `commitAccepted`（合并）+ 经 `s:<account>` `deliverWelcome`。
//      EpochStale    → `catchUpAndRerun`（拉取胜出链上 commit、重新暂存）并重试，有界。
//      其它错误       → `commitAbandoned`（丢弃暂存分叉）并上抛。
// 驱动经 `submitGroupCommit` 与实链解耦，故可对假 CAS + 真实 OpenMLS 执行器单测。

import type { WireCommitExecutor } from "@/mls/directAccountCommitCoordinator";
import type { CommitIntentControlMsg } from "@/mls/directCommitCoordination";

/// EN: Verdict of submitting a staged group commit to the chain CAS. CN: 把暂存群 commit 提交到链 CAS 的裁决。
export type GroupCommitSubmitResult =
  | { ok: true; newEpoch: number }
  | { ok: false; reason: "epoch_stale"; currentEpoch: number }
  | { ok: false; reason: "error"; error: unknown };

/// EN: Submit one staged group commit to the chain `commit` extrinsic at `expectedEpoch` (the CAS).
/// The implementation builds the extrinsic args from the engine's staged state (commit bytes + new
/// tree/transcript hashes + member_delta) and maps the dispatch result to a `GroupCommitSubmitResult`
/// (notably `EpochStale` → `{ ok:false, reason:"epoch_stale", currentEpoch }`). CN: 把一个暂存群 commit
/// 以 `expectedEpoch` 提交到链上 `commit` extrinsic（CAS）。实现从引擎暂存态构建 extrinsic 入参（commit 字节
/// + 新 tree/transcript hash + member_delta），并把派发结果映射为 `GroupCommitSubmitResult`（尤其
/// `EpochStale` → `{ ok:false, reason:"epoch_stale", currentEpoch }`）。
export type SubmitGroupCommit = (args: {
  intent: CommitIntentControlMsg;
  commitB64: string;
  expectedEpoch: number;
}) => Promise<GroupCommitSubmitResult>;

export interface GroupCommitOrderingDriverDeps {
  /** EN: Group device executor (`createGroupDeviceExecutor`). CN: 群设备执行器（`createGroupDeviceExecutor`）。 */
  executor: WireCommitExecutor;
  /** EN: Chain CAS submit (see `SubmitGroupCommit`). CN: 链 CAS 提交（见 `SubmitGroupCommit`）。 */
  submitGroupCommit: SubmitGroupCommit;
  /** EN: Max EpochStale retries before giving up (§16, default 5). CN: 放弃前的 EpochStale 重试上限
   *  （§16，默认 5）。 */
  maxRetries?: number;
}

/// EN: Outcome of driving a single group device-op to completion. CN: 把单个群设备操作驱动到完成的结果。
export interface GroupCommitOutcome {
  ok: boolean;
  /** EN: failure cause when `!ok`. CN: `!ok` 时的失败原因。 */
  reason?: "epoch_stale_exhausted" | "error";
  /** EN: number of chain submit attempts made. CN: 链提交尝试次数。 */
  attempts: number;
  /** EN: group epoch after a successful commit. CN: 成功 commit 后的群 epoch。 */
  finalEpoch?: number;
  /** EN: underlying error when `reason === "error"`. CN: `reason === "error"` 时的底层错误。 */
  error?: unknown;
}

const DEFAULT_MAX_RETRIES = 5;

/// EN: Drive one group device-op through the chain `expected_epoch` CAS with bounded EpochStale
/// catch-up + retry. Returns a terminal `GroupCommitOutcome`; never throws (executor/chain faults are
/// captured). CN: 把单个群设备操作经链上 `expected_epoch` CAS 驱动，附有界 EpochStale 追平 + 重试。返回终态
/// `GroupCommitOutcome`；绝不抛错（执行器/链故障被捕获）。
export class GroupCommitOrderingDriver {
  private readonly maxRetries: number;

  constructor(private deps: GroupCommitOrderingDriverDeps) {
    this.maxRetries = deps.maxRetries ?? DEFAULT_MAX_RETRIES;
  }

  async run(intent: CommitIntentControlMsg): Promise<GroupCommitOutcome> {
    const { executor, submitGroupCommit } = this.deps;

    let staged: { commitB64: string; welcomeB64: string; preEpoch: number };
    try {
      staged = await executor.runIntent(intent);
    } catch (error) {
      // EN: nothing staged on a runIntent failure → no commit to abandon. CN: runIntent 失败时未暂存
      // → 无 commit 需丢弃。
      return { ok: false, reason: "error", attempts: 0, error };
    }

    let expectedEpoch = staged.preEpoch;
    let commitB64 = staged.commitB64;
    let welcomeB64 = staged.welcomeB64;

    // attempts 1..=(maxRetries+1): the first submit plus up to `maxRetries` catch-up retries.
    for (let attempt = 1; attempt <= this.maxRetries + 1; attempt++) {
      let res: GroupCommitSubmitResult;
      try {
        res = await submitGroupCommit({ intent, commitB64, expectedEpoch });
      } catch (error) {
        await this.abandon(intent);
        return { ok: false, reason: "error", attempts: attempt, error };
      }

      if (res.ok) {
        // EN: chain accepted the slot → merge locally, then fan the device Welcome over s:<account>.
        // CN: 链已采纳该槽位 → 本地合并，再经 s:<account> 扇出设备 Welcome。
        try {
          await executor.commitAccepted(intent);
          await executor.deliverWelcome(intent, welcomeB64);
        } catch (error) {
          // EN: the chain epoch already advanced (irreversible); a local merge/deliver fault is
          // surfaced but the commit is NOT abandoned. CN: 链 epoch 已推进（不可逆）；本地合并/投递故障上抛，
          // 但**不**丢弃该 commit。
          return { ok: false, reason: "error", attempts: attempt, error, finalEpoch: res.newEpoch };
        }
        return { ok: true, attempts: attempt, finalEpoch: res.newEpoch };
      }

      if (res.reason === "error") {
        await this.abandon(intent);
        return { ok: false, reason: "error", attempts: attempt, error: res.error };
      }

      // EpochStale: someone else's commit won this epoch.
      if (attempt > this.maxRetries) {
        await this.abandon(intent);
        return { ok: false, reason: "epoch_stale_exhausted", attempts: attempt };
      }

      // EN: discard our forked staged commit, pull the winning chain commits up to currentEpoch, and
      // re-stage at the caught-up epoch. CN: 丢弃我方分叉暂存 commit，拉取胜出链上 commit 至 currentEpoch，
      // 并在追平后的 epoch 重新暂存。
      try {
        const rerun = await executor.catchUpAndRerun(intent, res.currentEpoch);
        expectedEpoch = rerun.newEpoch;
        commitB64 = rerun.commitB64;
        welcomeB64 = rerun.welcomeB64;
      } catch (error) {
        // EN: could not catch up (winning commits not applied locally yet) → give up this cycle; the
        // caller may re-run later. CN: 无法追平（胜出 commit 尚未本地应用）→ 放弃本轮；调用方可稍后重跑。
        await this.abandon(intent);
        return { ok: false, reason: "error", attempts: attempt, error };
      }
    }

    // Unreachable: the loop returns on accept, error, or exhaustion.
    await this.abandon(intent);
    return { ok: false, reason: "epoch_stale_exhausted", attempts: this.maxRetries + 1 };
  }

  private async abandon(intent: CommitIntentControlMsg): Promise<void> {
    try {
      await this.deps.executor.commitAbandoned(intent);
    } catch (e) {
      console.warn("[nexchat][group-wire] commit abandon failed:", e);
    }
  }
}
