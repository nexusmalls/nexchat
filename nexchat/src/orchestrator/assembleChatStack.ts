// EN: App-startup assembly for the 1:1 DR stack + 2↔3 orchestrator (design §11/§13). This is the
// single composition root that turns the app's existing singletons (relay, chain, MLS engine,
// local store) into a working DR engine, multi-device router, conversation-stack registry, and
// `ChatOrchestrator`. It is the ONE place that depends on both engines' ports + adapters; the
// engines themselves stay import-decoupled (enforced by `decoupling.test.ts`). Call it once per
// account at unlock, gated by `config.drEnabled` (default on when relay configured → DR-first 1:1).
// CN: 1:1 DR 栈 + 2↔3 编排器的 App 启动装配（设计 §11/§13）。这是唯一的组合根：把 App 既有单例
// （relay、chain、MLS 引擎、本地库）装配成可用的 DR 引擎、多设备路由、会话栈注册表与
// `ChatOrchestrator`。它是唯一同时依赖两引擎端口 + 适配器之处；引擎自身保持 import 解耦（由
// `decoupling.test.ts` 强制）。解锁时按账户调用一次，受 `config.drEnabled` 门控（已配置 relay 时
// 默认开启 → 1:1 优先 DR，MLS-Wire 回退）。

import { config } from "@/config";
import type { ChainClient } from "@/chain/chainClient";
import { DrTransport, restoreDrTransport } from "@/crypto-dr/drTransport";
import {
  ChainDeviceDirectory,
  MultiDeviceRouter,
  RelayBundleProvider,
} from "@/crypto-dr/multiDevice";
import {
  type ConvStackRegistry,
  MemoryConvStackRegistry,
} from "@/crypto-dr/convStack";
import {
  type DrPersistence,
  EncryptedDrSessionStore,
  MemoryDrSessionStore,
} from "@/crypto-dr/sessionStore";
import { OpkResponder } from "@/crypto-dr/opkExchange";
import type { VodozemacEngine } from "@/crypto-dr/vodozemacEngine";
import type { OpenMlsEngine } from "@/mls/openMlsEngine";
import { ChatOrchestrator } from "@/orchestrator/chatOrchestrator";
import { DrSessionAdapter } from "@/orchestrator/drSessionAdapter";
import { MlsGroupAdapter } from "@/orchestrator/mlsGroupAdapter";
import { MsgArchivePort } from "@/orchestrator/archiveAdapter";
import type { ArchivePusher } from "@/orchestrator/archiveAdapter";
import type { RelayClient } from "@/relay/relayClient";

/// EN: The assembled DR + orchestration stack handed back to app wiring. CN: 装配好的 DR + 编排
/// 栈，交还给 App 接线。
export interface ChatStack {
  /// EN: DR engine (Olm account + per-device ratchet sessions). CN: DR 引擎（Olm 账户 + 每设备棘轮会话）。
  engine: VodozemacEngine;
  /// EN: Single-device relay transport (`d:` delivery). CN: 单设备 relay 传输（`d:` 投递）。
  transport: DrTransport;
  /// EN: Account-level fan-out router (Scheme A). CN: 账户级扇出路由（方案 A）。
  router: MultiDeviceRouter;
  /// EN: 2↔3 transition state machine. CN: 2↔3 切换状态机。
  orchestrator: ChatOrchestrator;
  /// EN: Sticky per-peer DR / MLS-Wire stack pins (§20 二选一收口). CN: 每对端 DR / MLS-Wire 栈
  /// 钉定（§20 二选一收口）。
  stackRegistry: ConvStackRegistry;
  /// EN: DR at-rest persistence backing the engine. CN: 支撑引擎的 DR 静态持久化。
  drStore: DrPersistence;
  /// EN: OPK control-plane responder (single-dispenses one-time prekeys to X3DH initiators, §19);
  /// already `attach()`ed to the relay control channel. CN: OPK 控制面响应方（向 X3DH 发起方单发
  /// 一次性预密钥，§19）；已 `attach()` 到 relay 控制通道。
  opkResponder: OpkResponder;
}

const hex = (b: Uint8Array): string =>
  Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");

/// EN: Choose the DR persistence backend: vault-encrypted IndexedDB in a real browser, an
/// in-memory store under mock/SSR/no-IDB. Mirrors `localStore`'s selection (§17.2). CN: 选择 DR
/// 持久化后端：真实浏览器用 vault 加密 IndexedDB，mock/SSR/无 IDB 用内存。与 `localStore` 选择一致。
function makeDrStore(): DrPersistence {
  if (!config.useMock && typeof indexedDB !== "undefined") {
    return new EncryptedDrSessionStore();
  }
  return new MemoryDrSessionStore();
}

export interface AssembleChatStackOptions {
  /// EN: Canonical self account. CN: 规范本账户。
  account: string;
  relay: RelayClient;
  chain: ChainClient;
  /// EN: Group (MLS) engine used for 2→3 promotion / 3→2 dissolution. CN: 用于 2→3 升群 / 3→2
  /// 解散的群（MLS）引擎。
  mlsEngine: OpenMlsEngine;
  /// EN: Relay endpoint id; used as the OPK-fetch requester ref. CN: relay 端点 id；用作 OPK 取回
  /// 请求方 ref。
  endpointId: string;
  /// EN: Archive history flusher (defaults to the account's `MsgArchiveSync` via `localStore`).
  /// CN: 归档历史刷写器（默认经 `localStore` 取账户 `MsgArchiveSync`）。
  archivePusher: ArchivePusher;
  /// EN: Group name applied when a 1:1 is promoted to a group (2→3). CN: 1:1 升群（2→3）时所用群名。
  groupName?: string;
  /// EN: Override the DR persistence store (tests). CN: 覆盖 DR 持久化存储（测试）。
  drStore?: DrPersistence;
  /// EN: Override the stack registry (tests / persistent backing). CN: 覆盖栈注册表（测试 / 持久化）。
  stackRegistry?: ConvStackRegistry;
}

/// EN: Build the full DR + orchestration stack for `account`, restoring ratchet state from the
/// (encrypted) store when present. Safe to call once per unlock. CN: 为 `account` 构建完整 DR +
/// 编排栈，存在时从（加密）存储恢复棘轮态。每次解锁调用一次即可。
export async function assembleChatStack(opts: AssembleChatStackOptions): Promise<ChatStack> {
  const drStore = opts.drStore ?? makeDrStore();
  const { engine, transport } = await restoreDrTransport({
    account: opts.account,
    relay: opts.relay,
    store: drStore,
  });

  const directory = new ChainDeviceDirectory();
  // EN: Refresh the device roster cache when a peer device advertises prekeys (`opk_publish`), so
  // fan-out picks up newly-joined peer devices. CN: 对端设备发布预密钥（`opk_publish`）时刷新设备
  // 名册缓存，使扇出纳入新加入的对端设备。
  directory.subscribeRefresh(opts.relay);
  const provider = new RelayBundleProvider(opts.relay, opts.account);
  const router = new MultiDeviceRouter(transport, directory, provider);

  const drAdapter = new DrSessionAdapter(router);
  const mlsAdapter = new MlsGroupAdapter({
    engine: opts.mlsEngine,
    chain: opts.chain,
    selfAddress: opts.account,
    groupName: opts.groupName,
  });
  const archivePort = new MsgArchivePort(opts.account, { pusher: opts.archivePusher });
  const orchestrator = new ChatOrchestrator(drAdapter, mlsAdapter, archivePort);

  const stackRegistry = opts.stackRegistry ?? new MemoryConvStackRegistry();

  // EN: Serve OPK fetches for THIS device from the persisted bundle (populated by
  // `publishPrekeyBundle`). `onControl` is fan-out across all relay transports, so this coexists
  // with the MLS control consumers (`DirectMlsRegistry`). CN: 用持久化 bundle（由
  // `publishPrekeyBundle` 填充）服务本设备的 OPK 取回。`onControl` 在所有 relay 传输上为扇出，故与
  // MLS 控制消费者（`DirectMlsRegistry`）共存。
  const opkResponder = new OpkResponder(opts.relay, drStore, opts.account, hex(engine.deviceId()));
  opkResponder.attach();

  return { engine, transport, router, orchestrator, stackRegistry, drStore, opkResponder };
}
