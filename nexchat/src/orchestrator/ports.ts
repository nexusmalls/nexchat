// EN: Orchestrator ports (design §2/§3/§11). The 2↔3 transition state machine depends ONLY
// on these narrow public entry points of each engine — `create*` / `destroy*` / `isActive`
// — and NEVER on internal session stores, ratchet variables, or any key material (decoupling
// invariant §2.3). The DR/MLS/archive adapters implement these ports; the orchestrator is the
// single component allowed to hold both a DR and an MLS port at once.
// CN: 编排器端口（设计 §2/§3/§11）。2↔3 切换状态机**只**依赖各引擎这些窄公开入口
// （`create*` / `destroy*` / `isActive`），**绝不**触达内部会话库、棘轮变量或任何密钥材料
// （解耦不变量 §2.3）。DR/MLS/归档适配器实现这些端口；编排器是唯一可同时持有 DR 与 MLS 端口的组件。

/// EN: On-chain MLS group id. CN: 链上 MLS 群 id。
export type GroupId = number;

/// EN: The 1:1 Double Ratchet session control surface for one peer account (Scheme A:
/// fan-out across the peer's devices happens inside the adapter). The orchestrator only
/// freezes / resumes / retires / re-opens — it never reads ratchet state. CN: 针对单个对端
/// 账户的 1:1 双棘轮会话控制面（方案 A：对端多设备扇出在适配器内完成）。编排器只做
/// 冻结/恢复/退役/重开——绝不读棘轮态。
export interface DrSessionPort {
  /// EN: Establish (or re-X3DH) the DR session set with `peer` and mark it the active 1:1
  /// crypto (used on 3→2). CN: 与 `peer` 建立（或重新 X3DH）DR 会话集合并置为活跃 1:1 密码栈
  /// （3→2 用）。
  open(peer: string): Promise<void>;
  /// EN: Freeze: stop accepting NEW outbound sends and mark archived (REVERSIBLE — used at
  /// the start of 2→3 so it can be rolled back). CN: 冻结：停止接收**新**出站发送并标记归档
  /// （可逆——2→3 起始处，便于回滚）。
  freeze(peer: string): Promise<void>;
  /// EN: Undo `freeze` (rollback when the group create fails). CN: 撤销 `freeze`（建群失败时回滚）。
  resume(peer: string): Promise<void>;
  /// EN: Retire + destroy the DR session after a committed switch to group (irreversible
  /// cleanup; history stays in the archive). CN: 切群提交后退役 + 销毁 DR 会话（不可逆清理；
  /// 历史留在归档层）。
  retire(peer: string): Promise<void>;
  /// EN: Whether DR is the live 1:1 crypto for `peer` (active and not frozen). CN: DR 是否为
  /// `peer` 的活跃 1:1 密码栈（已激活且未冻结）。
  isActive(peer: string): boolean;
}

/// EN: The MLS group lifecycle control surface (the real adapter wraps `@/mls` group create /
/// dissolve; the orchestrator only calls these). CN: MLS 群生命周期控制面（真实适配器包裹
/// `@/mls` 群建/解散；编排器只调用这些）。
export interface MlsGroupPort {
  /// EN: Create an on-chain MLS group over `members` (KeyPackage/Welcome/Commit handled
  /// inside) and return its id. CN: 对 `members` 建链上 MLS 群（内部处理 KP/Welcome/Commit）
  /// 并返回 id。
  createGroup(members: string[]): Promise<GroupId>;
  /// EN: Dissolve the group on chain (3→2). CN: 链上解散群（3→2）。
  dissolve(groupId: GroupId): Promise<void>;
  /// EN: Whether the group is live. CN: 群是否存活。
  isActive(groupId: GroupId): boolean;
}

/// EN: History archive control surface (EISA `K_archive`, key-orthogonal to DR/MLS). The
/// orchestrator snapshots a conversation at every switch boundary so readable history
/// survives the crypto-stack change. CN: 历史归档控制面（EISA `K_archive`，与 DR/MLS 密钥
/// 正交）。编排器在每次切换边界对会话快照，使可读历史跨密码栈变更存活。
export interface ArchivePort {
  /// EN: Snapshot `convId` history into the archive (idempotent). CN: 把 `convId` 历史快照入
  /// 归档（幂等）。
  archive(convId: string): Promise<void>;
}
