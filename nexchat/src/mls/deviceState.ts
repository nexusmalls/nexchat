// EN: Track A device state machine + "credential not ready" classification (design
// CHAT_MULTIDEVICE_MLS_SYNC §5.6/§7.3). PURE logic only — no IO, no rendering. Two concerns:
//   (1) `deriveDeviceMode`: which of the three §7.3 states a device is in — `restoring` (importing
//       vault / catching up epoch), `primary` (holds authority + signing key → may send), or
//       `secondary` (read-only follower).
//   (2) `classifyCredentialReadiness`: the §5.6 fix for the original "凭证没有就绪" bug, which lumped
//       three very different situations into one opaque error. This splits them so the UI can show the
//       RIGHT next action instead of a dead end.
// CN: 路线 A 设备态机 + 「凭证未就绪」分类（设计 §5.6/§7.3）。仅**纯逻辑**——无 IO、无渲染。两个关注点：
//   (1) `deriveDeviceMode`：设备处于 §7.3 三态之一——`restoring`（装载 vault / 追平 epoch 中）、`primary`
//       （持权威 + 签名钥 → 可发送）、`secondary`（只读跟读）。
//   (2) `classifyCredentialReadiness`：§5.6 对原始「凭证没有就绪」bug 的修复——原实现把三种迥异情形混成一个
//       不透明错误；此处拆开，使 UI 能给出**正确**的下一步动作而非死胡同。

/// EN: The three device states (§7.3). CN: 三种设备态（§7.3）。
export type DeviceMode = "primary" | "secondary" | "restoring";

/// EN: Derive the device mode. `restoring` dominates (we don't know authority until catch-up
/// finishes); otherwise `primary` iff the device may actually send (authority + signing key, per
/// `sendingAuthority.canSend`), else `secondary`. Pure. CN: 推导设备态。`restoring` 优先（追平完成前
/// 无法确定权威）；否则当设备**确实可发送**（权威 + 签名钥，见 `sendingAuthority.canSend`）时为 `primary`，
/// 否则 `secondary`。纯函数。
export function deriveDeviceMode(args: { restoring: boolean; canSend: boolean }): DeviceMode {
  if (args.restoring) return "restoring";
  return args.canSend ? "primary" : "secondary";
}

/// EN: "Credential not ready" branches (§5.6). Each blocked branch names a DISTINCT cause + the single
/// action the UI should offer. CN: 「凭证未就绪」分支（§5.6）。每个 blocked 分支命名一个**不同**成因 +
/// UI 应提供的唯一动作。
export type CredentialReadiness =
  /// EN: device can read the group (has a usable local MLS session). CN: 设备可读该群（有可用本地 MLS 会话）。
  | { status: "ready" }
  /// EN: ① RPC unreachable — cannot determine state; retry. CN: ① RPC 未连通——无法判断，重试。
  | { status: "blocked"; branch: "rpc-disconnected"; action: "retry" }
  /// EN: ② a fresh group whose Welcome this device has not consumed yet → run the normal Welcome flow.
  /// CN: ② 本设备尚未消费 Welcome 的新群 → 走原 Welcome 流程。
  | { status: "blocked"; branch: "unclaimed-welcome"; action: "claim-welcome" }
  /// EN: ③a already a member, no local session, but a vault exists → restore from the escrow vault.
  /// CN: ③a 已是成员、无本地会话，但有 vault → 从托管 vault 恢复。
  | { status: "blocked"; branch: "restore-from-vault"; action: "restore-vault" }
  /// EN: ③b already a member, no local session AND no vault → §5.3 recovery (export bundle / re-add /
  /// External Join). CN: ③b 已是成员、无本地会话且无 vault → §5.3 兜底（导出恢复包 / 重邀 / External Join）。
  | { status: "blocked"; branch: "no-session-no-vault"; action: "fallback-recovery" }
  /// EN: not a member of this group and no pending Welcome — nothing to unlock here. CN: 非该群成员且
  /// 无待领 Welcome——此处无可解锁。
  | { status: "blocked"; branch: "not-a-member"; action: "await-invite" };

/// EN: Classify why a device cannot yet read a group, mapping to the §5.6 branches. Priority:
/// a usable local session ⇒ ready; else RPC must be up to judge; else a pending Welcome is the new-
/// group path; else (member without session) vault presence decides restore vs fallback; else the
/// device simply isn't in the group. Pure. CN: 分类设备为何尚不能读某群，映射到 §5.6 分支。优先级：
/// 有可用本地会话 ⇒ ready；否则需 RPC 在线才能判断；否则待领 Welcome 为新群路径；否则（成员但无会话）由
/// 是否有 vault 决定 restore 还是兜底；否则设备根本不在群内。纯函数。
export function classifyCredentialReadiness(args: {
  rpcConnected: boolean;
  hasLocalSession: boolean;
  hasPendingWelcome: boolean;
  joinedOnChain: boolean;
  hasVault: boolean;
}): CredentialReadiness {
  if (args.hasLocalSession) return { status: "ready" };
  if (!args.rpcConnected) return { status: "blocked", branch: "rpc-disconnected", action: "retry" };
  if (args.hasPendingWelcome) {
    return { status: "blocked", branch: "unclaimed-welcome", action: "claim-welcome" };
  }
  if (args.joinedOnChain) {
    return args.hasVault
      ? { status: "blocked", branch: "restore-from-vault", action: "restore-vault" }
      : { status: "blocked", branch: "no-session-no-vault", action: "fallback-recovery" };
  }
  return { status: "blocked", branch: "not-a-member", action: "await-invite" };
}
