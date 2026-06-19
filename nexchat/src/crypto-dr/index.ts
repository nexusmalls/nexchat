// EN: Public surface of the decentralized 1:1 stack (X3DH + Double Ratchet). Strictly
// decoupled from the OpenMLS group stack — this barrel MUST NOT re-export anything from
// `@/mls/*` (decoupling invariant, design §2/§3). CN: 去中心化 1:1 栈（X3DH + 双棘轮）的
// 公开面。与 OpenMLS 群栈严格解耦——本 barrel 不得从 `@/mls/*` 再导出任何东西（解耦不变量，
// 设计 §2/§3）。

export * from "@/crypto-dr/types";
export {
  DM_ENVELOPE_VER,
  deviceIdFromIk,
  encodeDmEnvelope,
  decodeDmEnvelope,
  peekDmHeader,
} from "@/crypto-dr/dmEnvelope";
export { VodozemacEngine, ensureDrWasm } from "@/crypto-dr/vodozemacEngine";
export {
  opkMerkleRoot,
  opkMerkleProof,
  verifyOpkProof,
  encodeOpkProof,
  decodeOpkProof,
  sortOpks,
  type OpkProofStep,
} from "@/crypto-dr/opkMerkle";
export {
  EncryptedDrSessionStore,
  MemoryDrSessionStore,
  type DrPersistence,
  type PublishedOpkBundle,
} from "@/crypto-dr/sessionStore";
export {
  OpkResponder,
  dispenseOpkLeaf,
  requestOpk,
  fetchPeerBundleWithOpk,
  type ServedOpkLeaf,
  type RequestOpkOptions,
} from "@/crypto-dr/opkExchange";
export {
  CTX_IK_ENDORSE,
  CTX_SPK_ENDORSE,
  DR_STACK_VERSION,
  endorseKey,
  verifyEndorsement,
  publishPrekeyBundle,
  advertiseStackCaps,
  type PublishOptions,
  type PublishedBundle,
} from "@/crypto-dr/identityBridge";
export {
  assemblePeerBundle,
  fetchPeerBundle,
  chooseStack,
  negotiateStack,
  type FetchBundleOptions,
  type StackChoice,
} from "@/crypto-dr/prekeyFetch";
export {
  MemoryConvStackRegistry,
  resolveConvStack,
  type ConvStackRegistry,
  type ResolveConvStackOptions,
} from "@/crypto-dr/convStack";
export {
  DrTransport,
  restoreDrTransport,
  type DrIncoming,
  type DrMessageHandler,
} from "@/crypto-dr/drTransport";
export {
  MultiDeviceRouter,
  ChainDeviceDirectory,
  RelayBundleProvider,
  type DeviceDirectory,
  type PeerBundleProvider,
  type MultiDeviceSendResult,
} from "@/crypto-dr/multiDevice";
