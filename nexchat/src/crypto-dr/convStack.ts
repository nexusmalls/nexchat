// EN: 1:1 conversation stack registry (design §12 / §20, milestone M3 "二选一收口"). A
// given pairwise conversation MUST be pinned to exactly ONE crypto stack — either the
// decentralized Double Ratchet (`dr`) or pairwise MLS-Wire (`mls_wire`) — and never carry
// both states at once (§2 invariant). This module is the small, engine-agnostic decision +
// persistence layer that pins the chosen stack on first contact (after §20 capability
// negotiation) and returns the pinned choice thereafter, so a conversation never silently
// switches stacks mid-life. Import-decoupled from both `@/mls/*` and the DR engine.
// CN: 1:1 会话栈注册表（设计 §12 / §20，里程碑 M3「二选一收口」）。任一成对会话必须**恰好**
// 钉在一种密码栈——去中心化双棘轮（`dr`）或 pairwise MLS-Wire（`mls_wire`）——绝不同时持有
// 两种状态（§2 不变量）。本模块是引擎无关的小型决策 + 持久化层：首次联系时（经 §20 能力协商）
// 钉定所选栈，此后返回钉定值，使会话不会中途静默切栈。与 `@/mls/*` 及 DR 引擎 import 解耦。

import { negotiateStack, type StackChoice } from "@/crypto-dr/prekeyFetch";
import { STACK_DR, STACK_MLS_WIRE } from "@/crypto-dr/types";
import { canonicalAddress } from "@/wallet/address";

/// EN: Per-peer chosen-stack persistence (keyed by canonical peer account). Implementations
/// may back this with the conversation index / encrypted local store. CN: 每对端所选栈的
/// 持久化（以规范对端账户为键）。实现可落到会话索引 / 加密本地库。
export interface ConvStackRegistry {
  get(peer: string): Promise<StackChoice | null>;
  set(peer: string, stack: StackChoice): Promise<void>;
}

/// EN: In-memory registry (tests / ephemeral). CN: 内存注册表（测试 / 临时）。
export class MemoryConvStackRegistry implements ConvStackRegistry {
  private map = new Map<string, StackChoice>();
  async get(peer: string): Promise<StackChoice | null> {
    return this.map.get(canonicalAddress(peer)) ?? null;
  }
  async set(peer: string, stack: StackChoice): Promise<void> {
    this.map.set(canonicalAddress(peer), stack);
  }
}

/// EN: Options for `resolveConvStack`. CN: `resolveConvStack` 选项。
export interface ResolveConvStackOptions {
  /// EN: This client's supported stack flags (default DR + MLS-Wire). CN: 本客户端支持的栈
  /// 位（默认 DR + MLS-Wire）。
  selfFlags?: number;
  /// EN: Force re-negotiation, overwriting any existing pin (e.g. peer upgraded). Use with
  /// care — re-pinning a live conversation to a different stack is a migration, not a
  /// routine read. CN: 强制重新协商并覆盖既有钉定（如对端升级）。慎用——把活跃会话重钉到
  /// 另一栈属迁移而非常规读取。
  renegotiate?: boolean;
  /// EN: Injectable negotiator (defaults to the §20 chain-backed `negotiateStack`); override
  /// in tests. CN: 可注入的协商器（默认 §20 链上 `negotiateStack`）；测试可覆盖。
  negotiate?: (peer: string, selfFlags?: number) => Promise<StackChoice>;
}

/// EN: Resolve (and pin) the crypto stack for a 1:1 conversation with `peer`. Returns the
/// existing pin if any (二选一收口); otherwise negotiates per §20 and pins the result. An
/// incompatible result (`"none"`) is NOT pinned, so a later attempt can succeed once the
/// peer upgrades. CN: 解析（并钉定）与 `peer` 的 1:1 会话密码栈。已钉定则返回（二选一收口）；
/// 否则按 §20 协商并钉定结果。不兼容（`"none"`）不钉定，使对端升级后可重试成功。
export async function resolveConvStack(
  peer: string,
  registry: ConvStackRegistry,
  opts: ResolveConvStackOptions = {},
): Promise<StackChoice> {
  const selfFlags = opts.selfFlags ?? STACK_DR | STACK_MLS_WIRE;
  const negotiate = opts.negotiate ?? negotiateStack;

  if (!opts.renegotiate) {
    const pinned = await registry.get(peer);
    if (pinned && pinned !== "none") return pinned;
  }

  const choice = await negotiate(peer, selfFlags);
  if (choice !== "none") await registry.set(peer, choice);
  return choice;
}
