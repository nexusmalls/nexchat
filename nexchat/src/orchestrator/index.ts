// EN: Public surface of the 2↔3 transition orchestrator (design §11). This is the SINGLE
// module allowed to depend on both the DR (`@/crypto-dr/*`) and MLS (`@/mls/*`) engines; it
// drives switches via their public ports only and never touches key state (§2.3). CN: 2↔3
// 切换编排器公开面（设计 §11）。这是唯一可同时依赖 DR（`@/crypto-dr/*`）与 MLS（`@/mls/*`）
// 引擎的模块；只经公开端口驱动切换，绝不触碰密钥态（§2.3）。

export {
  ChatOrchestrator,
  type ConvMode,
  type ConvState,
  type SwitchResult,
} from "@/orchestrator/chatOrchestrator";
export type { ArchivePort, DrSessionPort, GroupId, MlsGroupPort } from "@/orchestrator/ports";
export { DrSessionAdapter } from "@/orchestrator/drSessionAdapter";
export { MlsGroupAdapter, type MlsGroupAdapterDeps } from "@/orchestrator/mlsGroupAdapter";
export { MsgArchivePort, type ArchivePusher } from "@/orchestrator/archiveAdapter";
export {
  assembleChatStack,
  type AssembleChatStackOptions,
  type ChatStack,
} from "@/orchestrator/assembleChatStack";
