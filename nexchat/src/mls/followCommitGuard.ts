// EN: Member-side E2EI re-verification for Wire (CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC §3.9 for
// 1:1; CHAT_GROUP_WIREIFY_DESIGN §6.4 for groups). When a FOLLOWER processes an incoming Commit, it
// independently confirms that EVERY leaf the Commit ADDS is account-bound — its in-MLS binding is
// signed by the SS58 key of the account it claims AND that account is ALLOWED in this conversation —
// instead of blindly trusting the committer's add-time check. The "allowed" policy differs by conv
// kind: a 1:1 (`d:`) admits only the two conv parties; a group (`g:`) admits only current on-chain
// members. This closes the residual gap on the follow path: a malicious/buggy committer that admits a
// foreign or wrongly-labelled leaf is caught here, relay-trustlessly, before the follower merges.
//
// CN: Wire 的成员侧 E2EI 复验（1:1 见串行化规范 §3.9；群见群 Wire 化设计 §6.4）。当**跟随者**处理进入 Commit
// 时，独立确认该 Commit **新增**的**每个** leaf 都账户绑定——其 MLS 内绑定由所声称账户的 SS58 钥签名，且该账户
// 在本会话中**被允许**——而非盲信提交方 add 时的校验。「被允许」策略按会话类型不同：1:1（`d:`）仅纳两方；
// 群（`g:`）仅纳当前链上成员。弥补跟随路径残余缺口：恶意/有缺陷提交方混入外来或错标 leaf，会在跟随者合并前于
// 此被 relay-trustless 拦下。

import { verifyLeafKeyBinding } from "@/mls/deviceLeafCredential";
import {
  accountFromLeafIdentity,
  deviceFromLeafIdentity,
  directMlsKeyInvolves,
} from "@/mls/directConv";

/// EN: Hex (`0x…`) encoding of raw bytes for `signatureVerify`. CN: 原始字节的 hex（`0x…`）编码，供
/// `signatureVerify` 使用。
export function bytesToHex(bytes: Uint8Array): string {
  let s = "0x";
  for (const b of bytes) s += b.toString(16).padStart(2, "0");
  return s;
}

/// EN: Engine surface needed for member-side commit inspection (both optional → engines without the
/// §3.9 primitives are treated as "no inspection", preserving prior behavior). CN: 成员侧 commit 检视
/// 所需的引擎接口（均可选 → 无 §3.9 原语的引擎视为「不检视」，保持原行为）。
export interface CommitInspectEngine {
  inspectCommitBindings?: (
    conv: string,
    commit: Uint8Array,
  ) => Array<{ identity: string; signatureKey: Uint8Array; binding: Uint8Array }>;
  discardIncomingCommit?: (conv: string) => void;
}

/// EN: Decide whether an incoming Commit on `conv` may be merged. Returns `true` to proceed (verified,
/// or the engine carries no inspection / the Commit adds no bound leaf → backward compatible) and
/// `false` to REJECT (binding verification failed, policy violation, or `inspectCommitBindings`
/// failed — malformed / epoch mismatch / staging conflict). Bindings are an additive hardening, so an
/// ABSENT binding (older peer engine) is allowed, mirroring the add-time policy in §3.7/§3.8.
/// CN: 判定 `conv` 上进入的 Commit 是否可合并。返回 `true` 放行（已验证，或引擎不支持检视 / 该 Commit 未加
/// 绑定 leaf → 向后兼容），`false` 拒绝（绑定验证失败、策略违例，或 `inspectCommitBindings` 失败——畸形 /
/// epoch 不符 / 暂存冲突）。绑定为加性硬化，故**缺失**绑定（旧版对端引擎）放行，与 §3.7/§3.8 add 时策略一致。
export async function verifyIncomingCommit(
  engine: CommitInspectEngine,
  conv: string,
  commit: Uint8Array,
): Promise<boolean> {
  // EN: 1:1 policy — an added leaf's account must be one of the two pairwise parties. CN: 1:1 策略——
  // 被加 leaf 的账户须是成对会话两方之一。
  return verifyIncomingCommitWithPolicy(engine, conv, commit, (acct) =>
    directMlsKeyInvolves(conv, acct),
  );
}

/// EN: Group variant (§6.4): an added device leaf's account must be a CURRENT group member. `isMember`
/// is the membership predicate (backed by the chain `GroupMembers` set / the local roster), so this
/// stays decoupled from chain access and unit-testable. CN: 群变体（§6.4）：被加设备 leaf 的账户须是
/// **当前**群成员。`isMember` 为成员判定谓词（由链上 `GroupMembers` 集 / 本地名册支撑），故与链访问解耦、
/// 可单测。
export async function verifyIncomingGroupCommit(
  engine: CommitInspectEngine,
  conv: string,
  commit: Uint8Array,
  isMember: (account: string) => boolean,
): Promise<boolean> {
  return verifyIncomingCommitWithPolicy(engine, conv, commit, isMember);
}

/// EN: Policy-driven core: verify each ADDED leaf's E2EI binding AND that its account passes
/// `isAllowedAccount`. Returns `false` (and discards any staged commit) on the first violation or
/// when `inspectCommitBindings` throws (do NOT bypass E2EI by falling through to blind `processCommit`).
/// An ABSENT binding (older peer engine) is allowed, mirroring the add-time policy. CN: 策略驱动核心：
/// 校验每个**新增** leaf 的 E2EI 绑定，且其账户通过 `isAllowedAccount`。首个违例或 `inspectCommitBindings`
/// 抛错时返回 `false`（并丢弃暂存 commit；**不得**盲走 `processCommit` 绕过 E2EI）。**缺失**绑定（旧版对端
/// 引擎）放行，与 add 时策略一致。
export async function verifyIncomingCommitWithPolicy(
  engine: CommitInspectEngine,
  conv: string,
  commit: Uint8Array,
  isAllowedAccount: (account: string) => boolean,
): Promise<boolean> {
  const inspect = engine.inspectCommitBindings;
  if (!inspect) return true; // engine without member-side inspection → plain processing
  let added: Array<{ identity: string; signatureKey: Uint8Array; binding: Uint8Array }>;
  try {
    added = inspect.call(engine, conv, commit);
  } catch {
    // EN: inspect failed (malformed, epoch mismatch, already applied, staging conflict, …) — treat as
    // REJECT; do not bypass member-side E2EI by deferring to blind processCommit. CN: inspect 失败
    // （畸形、epoch 不符、已应用、暂存冲突等）——视为**拒绝**；不得借 blind processCommit 绕过成员侧 E2EI。
    engine.discardIncomingCommit?.(conv);
    return false;
  }
  for (const a of added) {
    if (a.binding.length === 0) continue; // unbound (older peer) → allow, consistent with add-time
    const acct = accountFromLeafIdentity(a.identity);
    const ok =
      isAllowedAccount(acct) &&
      (await verifyLeafKeyBinding(
        acct,
        deviceFromLeafIdentity(a.identity),
        a.signatureKey,
        bytesToHex(a.binding),
      ));
    if (!ok) {
      engine.discardIncomingCommit?.(conv);
      return false;
    }
  }
  return true;
}
