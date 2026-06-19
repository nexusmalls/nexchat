// EN: MsgArchivePort — binds the orchestrator's `ArchivePort` to the real EISA `K_archive`
// history pipeline (`MsgArchiveSync.push`, which builds + encrypts + pins the archive blob and
// publishes the pointer). At every 2↔3 switch boundary the orchestrator snapshots history so
// readable text self-heals after the crypto stack changes, key-orthogonally (§11). `push` is
// account-wide and idempotent — the per-conversation `convId` is informational only. CN:
// MsgArchivePort —— 把编排器 `ArchivePort` 绑定到真实 EISA `K_archive` 历史管线
// （`MsgArchiveSync.push`：构建 + 加密 + 固定归档 blob 并发布指针）。编排器在每次 2↔3 切换边界
// 对历史快照，使密码栈变更后可读正文与密钥正交地自愈（§11）。`push` 为账户级且幂等——每会话
// `convId` 仅作信息。

import type { ArchivePort } from "@/orchestrator/ports";
import type { LocalStore } from "@/store/localStore";
import { msgArchiveSyncFor } from "@/store/msgArchiveSync";

/// EN: Minimal pusher surface (subset of `MsgArchiveSync`) so this port is unit-testable and
/// not coupled to the singleton. CN: 最小推送面（`MsgArchiveSync` 子集），使本端口可单测且不耦合
/// 单例。
export interface ArchivePusher {
  push(account: string): Promise<void>;
}

export class MsgArchivePort implements ArchivePort {
  private readonly pusher: ArchivePusher;

  constructor(
    private readonly account: string,
    deps: { store: LocalStore } | { pusher: ArchivePusher },
  ) {
    this.pusher = "pusher" in deps ? deps.pusher : msgArchiveSyncFor(deps.store);
  }

  /// EN: Flush the current history snapshot into `K_archive` before the stack switch.
  /// CN: 在栈切换前把当前历史快照刷入 `K_archive`。
  async archive(_convId: string): Promise<void> {
    await this.pusher.push(this.account);
  }
}
