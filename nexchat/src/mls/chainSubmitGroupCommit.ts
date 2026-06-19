// EN: Live-chain `SubmitGroupCommit` for the group Wire-ification ordering driver (CHAT_GROUP_WIREIFY_
// DESIGN §7 / §15.3 / G3c). Submits a STAGED group device commit to `pallet-chat-group::commit`, whose
// `expected_epoch` check IS the atomic CAS (block total order). It carries the POST-commit
// `(tree_hash, transcript_hash)` read from the staged commit via `stagedCommitFingerprintByConv` (G3b,
// §7.2) — the true new-epoch commitments, obtained WITHOUT a speculative merge. A Wire DEVICE commit
// (`add_device` / `remove_device` of a sibling / `rekey`) is an EMPTY-`member_delta` commit: the
// account roster is unchanged (only the per-device leaf set changes, opaque inside the commit bytes),
// so `member_delta = { added: [], removed: [] }` and `welcomes = []` (the joining device's Welcome is
// fanned off-chain over `s:<account>`, not on-chain). The chain accepts empty-delta commits without an
// owner role (§7.1; proven by `same_account_empty_delta_commit_rekey_is_allowed`).
//
// CN: 群 Wire 化定序驱动的实链 `SubmitGroupCommit`（设计 §7 / §15.3 / G3c）。把**已暂存**的群设备 commit
// 提交到 `pallet-chat-group::commit`，其 `expected_epoch` 检查**即**原子 CAS（区块全序）。携带经
// `stagedCommitFingerprintByConv`（G3b，§7.2）从暂存 commit 读出的**后置** `(tree_hash, transcript_hash)`
// ——真实新 epoch 承诺，**无需**投机合并即可得。Wire **设备** commit（`add_device` / 兄弟 `remove_device` /
// `rekey`）是**空-`member_delta`** commit：账户名册不变（仅每设备 leaf 集变化，藏于 commit 字节内），故
// `member_delta = { added: [], removed: [] }`、`welcomes = []`（加入设备的 Welcome 经 `s:<account>` 链下扇出，
// 不上链）。链对空-delta commit 无需群主角色放行（§7.1；由 `same_account_empty_delta_commit_rekey_is_allowed`
// 证明）。

import { hex } from "@/mls/chainBytes";
import type {
  GroupCommitSubmitResult,
  SubmitGroupCommit,
} from "@/mls/groupCommitOrderingDriver";
import { b64ToBytes } from "@/relay/relayClient";

/// EN: Minimal chain surface this factory needs (a subset of `ChainClient`), so it is unit-testable
/// against a fake. CN: 本工厂所需的最小链接口（`ChainClient` 子集），便于对假件单测。
export interface GroupCommitChain {
  /** EN: Submit an extrinsic and resolve on inclusion, or reject with `…failed: <section>.<Error>`.
   *  CN: 提交 extrinsic，上块即 resolve，失败以 `…failed: <section>.<Error>` reject。 */
  signAndSendDev(section: string, method: string, args: unknown[]): Promise<string>;
  /** EN: Read the on-chain group epoch (used to report the winner's `currentEpoch` on EpochStale).
   *  CN: 读链上群 epoch（EpochStale 时上报胜出者 `currentEpoch`）。 */
  groupSnapshot(groupId: number): Promise<{ epoch: number; memberCount: number } | null>;
}

/// EN: Engine surface: the G3b staged post-commit fingerprint reader. CN: 引擎接口：G3b 暂存后置指纹读取。
export interface StagedFingerprintEngine {
  stagedCommitFingerprintByConv(convKey: string): {
    treeHash: Uint8Array;
    transcriptHash: Uint8Array;
    epoch: number;
  };
}

export interface ChainSubmitGroupCommitDeps {
  chain: GroupCommitChain;
  engine: StagedFingerprintEngine;
  /** EN: Opaque `new_group_info_cid` placeholder (the chain stores it without interpreting it; mirrors
   *  `chainHandshake`'s 1-byte marker). CN: 不透明 `new_group_info_cid` 占位（链不解释，仅存储；与
   *  `chainHandshake` 的 1 字节标记一致）。 */
  groupInfoCid?: Uint8Array;
}

/// EN: Parse `g:<id>` → numeric group id. Throws on a non-group conv (the driver only runs `g:` convs).
/// CN: 解析 `g:<id>` → 数字群 id。非群会话抛错（驱动只跑 `g:` 会话）。
export function groupIdOfConv(convId: string): number {
  if (!convId.startsWith("g:")) {
    throw new Error(`chainSubmitGroupCommit: expected a group conv (g:…), got ${convId}`);
  }
  const id = Number(convId.slice(2));
  if (!Number.isInteger(id) || id < 0) {
    throw new Error(`chainSubmitGroupCommit: malformed group id in ${convId}`);
  }
  return id;
}

/// EN: True when a chain dispatch error is `EpochStale` (the anti-fork CAS lost the race). CN: 链派发
/// 错误为 `EpochStale`（防分叉 CAS 落败）时为真。
export function isEpochStaleError(e: unknown): boolean {
  const msg = e instanceof Error ? e.message : String(e);
  return msg.includes("EpochStale");
}

/// EN: Build a live-chain `SubmitGroupCommit` for `GroupCommitOrderingDriver`. CN: 为
/// `GroupCommitOrderingDriver` 构造实链 `SubmitGroupCommit`。
export function createChainSubmitGroupCommit(
  deps: ChainSubmitGroupCommitDeps,
): SubmitGroupCommit {
  return async ({ intent, commitB64, expectedEpoch }): Promise<GroupCommitSubmitResult> => {
    const convId = intent.payload.dmConvId;
    let gid: number;
    try {
      gid = groupIdOfConv(convId);
    } catch (error) {
      return { ok: false, reason: "error", error };
    }

    // EN: post-commit commitments from the STILL-staged (un-merged) commit (G3b). CN: 取**仍暂存**
    // （未合并）commit 的后置承诺（G3b）。
    let fp: { treeHash: Uint8Array; transcriptHash: Uint8Array; epoch: number };
    try {
      fp = deps.engine.stagedCommitFingerprintByConv(convId);
    } catch (error) {
      return { ok: false, reason: "error", error };
    }

    const cid = deps.groupInfoCid ?? new Uint8Array([2]);
    try {
      await deps.chain.signAndSendDev("chatGroup", "commit", [
        gid,
        expectedEpoch,
        hex(b64ToBytes(commitB64)),
        hex(fp.treeHash),
        hex(fp.transcriptHash),
        hex(cid),
        [], // EN: no on-chain Welcome (off-chain over s:<account>). CN: 无上链 Welcome（链下经 s:<account>）。
        { added: [], removed: [] }, // EN: empty-delta device commit. CN: 空-delta 设备 commit。
      ]);
      // EN: fp.epoch is the post-commit epoch (= expectedEpoch + 1). CN: fp.epoch 即后置 epoch（=expectedEpoch+1）。
      return { ok: true, newEpoch: fp.epoch };
    } catch (error) {
      if (isEpochStaleError(error)) {
        // EN: report the winner's epoch so the driver catches up + re-stages. CN: 上报胜出者 epoch，
        // 供驱动追平 + 重新暂存。
        const snap = await deps.chain.groupSnapshot(gid).catch(() => null);
        return { ok: false, reason: "epoch_stale", currentEpoch: snap?.epoch ?? expectedEpoch };
      }
      return { ok: false, reason: "error", error };
    }
  };
}
