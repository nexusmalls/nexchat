// EN: Runtime config sourced from Vite env (.env.example). CN: 来自 Vite env 的运行时配置。

// EN: Per-tab dev account seed. A `?as=//Bob` URL query wins over the env default so two
// browser tabs can each drive a distinct on-chain identity in the chain control-plane demo.
// CN: 每标签页 dev 账户种子。URL 查询 `?as=//Bob` 优先于 env 默认值，使两个浏览器标签页在链上
// 控制面演示中各自驱动一个独立链上身份。
function readDevSeed(): string {
  const fromEnv = import.meta.env.VITE_DEV_SEED ?? "//Alice";
  if (typeof window === "undefined") return fromEnv;
  const q = new URLSearchParams(window.location.search).get("as");
  return q && q.length > 0 ? q : fromEnv;
}

export const config = {
  wsEndpoint: import.meta.env.VITE_WS_ENDPOINT ?? "ws://127.0.0.1:9944",
  httpEndpoint: import.meta.env.VITE_HTTP_ENDPOINT ?? "http://127.0.0.1:9944",
  relayWs: import.meta.env.VITE_RELAY_WS ?? "",
  // EN: When true, attach sr25519 `account_sig` on `register_account` (required when relay runs
  // `RELAY_STRICT_AUTH=1`). Off by default on production relay — omitting sig avoids spurious
  // rejects if signing is unavailable; P1 relay verifies with `substrate` context when sig is sent.
  // CN: 为 true 时在 `register_account` 附带 sr25519 `account_sig`（relay 开 `RELAY_STRICT_AUTH=1`
  // 时必填）。生产 relay 默认关闭——省略 sig 可避免签名不可用时误拒；relay P1 在收到 sig 时用
  // `substrate` 上下文验签。
  relayStrictAuth: (import.meta.env.VITE_RELAY_STRICT_AUTH ?? "false") === "true",
  useMock: (import.meta.env.VITE_USE_MOCK ?? "true") === "true",
  mlsEnabled: (import.meta.env.VITE_MLS_ENABLED ?? "true") === "true",
  appName: import.meta.env.VITE_APP_NAME ?? "NexChat",
  // EN: crypto backend — "openmls" uses the real RFC 9420 WASM engine (with relay
  // handshake) for the demo group; "webcrypto" keeps the AES-GCM placeholder.
  // CN: 密码学后端——"openmls" 对 demo 群用真实 RFC 9420 WASM 引擎（含 relay 握手）；
  // "webcrypto" 保持 AES-GCM 占位。
  mlsBackend: (import.meta.env.VITE_MLS_BACKEND ?? "openmls") as "openmls" | "webcrypto",
  // EN: group id that multi-tab OpenMLS handshake binds to. CN: 多标签页 OpenMLS 握手绑定的群 id。
  mlsDemoGroupId: Number(import.meta.env.VITE_MLS_DEMO_GROUP ?? "0"),
  // EN: account used in live (non-mock) mode for read-only demo (no extension).
  // CN: 实时（非 mock）模式下用于只读演示的账户（无需扩展）。default = //Bob.
  devAddress:
    import.meta.env.VITE_DEV_ADDRESS ?? "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty",
  // EN: handshake control-plane — "chain" uses the real on-chain DS/AS (publish_key_package /
  // commit / pending_welcome); "relay" uses the BroadcastChannel simulation (mock/offline).
  // CN: 握手控制面——"chain" 用真实链上 DS/AS（publish_key_package/commit/pending_welcome）；
  // "relay" 用 BroadcastChannel 模拟（mock/离线）。
  mlsControlPlane: (import.meta.env.VITE_MLS_CONTROL_PLANE ??
    ((import.meta.env.VITE_USE_MOCK ?? "true") === "true" ? "relay" : "chain")) as
    | "chain"
    | "relay",
  // EN: this tab's dev signer seed (chain control-plane). CN: 本标签页 dev 签名种子（链上控制面）。
  devSeed: readDevSeed(),
  // EN: demo roster seeds; roster[0] is the group owner. Empty in production (VITE_MLS_ROSTER=).
  // CN: 演示名册种子；roster[0] 为群主。生产环境留空（VITE_MLS_ROSTER=）。
  mlsRosterSeeds: (import.meta.env.VITE_MLS_ROSTER ?? "//Alice,//Bob,//Charlie")
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0),
  // EN: SS58 default friends auto-added on unlock; labels from on-chain nickname (see
  // `ensureDefaultContacts`). Also merges `mlsRosterSeeds` addresses in dev when roster is set.
  // CN: 解锁时自动加入的默认好友 SS58；显示名优先链上昵称（见 `ensureDefaultContacts`）。开发环境
  // 配置了 `mlsRosterSeeds` 时也会并入其地址。
  defaultContactAddresses: (import.meta.env.VITE_DEFAULT_CONTACTS ?? "")
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0),
  // EN: IPFS — kubo API + gateway. Vite dev proxies `/ipfs-api` → :5001, `/ipfs-gateway` → :8080.
  // CN: IPFS——kubo API + 网关。Vite 开发把 `/ipfs-api` 代理到 :5001、`/ipfs-gateway` 到 :8080。
  ipfsEnabled: (import.meta.env.VITE_IPFS_ENABLED ?? "true") === "true",
  ipfsApiUrl: import.meta.env.VITE_IPFS_API_URL ?? "/ipfs-api",
  ipfsGatewayUrl: import.meta.env.VITE_IPFS_GATEWAY_URL ?? "/ipfs-gateway",
  // EN: chunked path when size > threshold or mime is video (CHAT_LARGE_FILE_SPEC.md §4).
  // CN: 超过阈值或视频 MIME 走分块路径（大文件规范 §4）。
  ipfsChunkThreshold: Number(import.meta.env.VITE_IPFS_CHUNK_THRESHOLD ?? String(16 * 1024 * 1024)),
  ipfsChunkSize: Number(import.meta.env.VITE_IPFS_CHUNK_SIZE ?? String(1024 * 1024)),
  ipfsThumbMaxPx: Number(import.meta.env.VITE_IPFS_THUMB_MAX_PX ?? "320"),
  // EN: request on-chain Pin after IPFS upload (costs balance; off by default).
  // CN: IPFS 上传后请求链上 Pin（消耗余额；默认关闭）。
  ipfsPinEnabled: (import.meta.env.VITE_IPFS_PIN_ENABLED ?? "false") === "true",
  // EN: pin chat media on the sender's local kubo (not chain/global). Ephemeral uploads
  // always skip pin. Sync blobs (conv-index / contacts / archive) always pin locally.
  // CN: 聊天媒体在本机 kubo pin（非链上/全局）。阅后即焚始终不 pin。sync blob 仍始终本机 pin。
  ipfsMediaLocalPinEnabled:
    (import.meta.env.VITE_IPFS_MEDIA_LOCAL_PIN ?? "true") === "true",
  // EN: sender local pin TTL in ms before best-effort unpin (default 30 days).
  // CN: 发送方本机 pin 保留时长（毫秒），到期尽力 unpin（默认 30 天）。
  ipfsMediaLocalPinTtlMs: Number(
    import.meta.env.VITE_IPFS_MEDIA_LOCAL_PIN_TTL_MS ?? String(30 * 24 * 60 * 60_000),
  ),
  // EN: encrypted conv-index blob sync via IPFS + relay pointer (CHAT_P2 §2.1).
  // CN: 经 IPFS + relay 指针同步加密 conv-index blob（CHAT_P2 §2.1）。
  convIndexEnabled: (import.meta.env.VITE_CONV_INDEX_ENABLED ?? "true") === "true",
  // EN: encrypted contact-book vault sync via IPFS + relay pointer (conv-index pattern).
  // CN: 经 IPFS + relay 指针同步加密通讯录 vault（与 conv-index 同构）。
  contactsVaultEnabled: (import.meta.env.VITE_CONTACTS_VAULT_ENABLED ?? "true") === "true",
  // EN: encrypted message-history archive sync via IPFS + relay pointer (P2 extension).
  // CN: 经 IPFS + relay 指针同步加密消息历史归档（P2 扩展）。
  msgArchiveEnabled: (import.meta.env.VITE_MSG_ARCHIVE_ENABLED ?? "true") === "true",
  msgArchiveMaxPerConv: Number(import.meta.env.VITE_MSG_ARCHIVE_MAX_PER_CONV ?? "500"),
  // EN: Track A — encrypted MLS state-escrow vault sync via IPFS + relay pointer (design
  // CHAT_MULTIDEVICE_MLS_SYNC §4/§13). DEFAULT ON (opt-out): a full device escrows its
  // signature-key-stripped group MLS state (sealed → IPFS → relay/chain pointer); a cold/swapped
  // device restores READ-ONLY group state then obtains sending authority via an online handoff (§5).
  // Requires ipfs. Set VITE_MLS_VAULT_ENABLED=false to disable.
  // CN: 路线 A —— 经 IPFS + relay 指针同步加密 MLS 状态托管 vault（设计 §4/§13）。默认开启（可关）：
  // 完整设备托管其剥离签名钥的群 MLS 状态（封装→IPFS→relay/链指针）；冷/换机设备恢复只读群状态后经
  // 在线交接（§5）获取发送权。需 ipfs。置 VITE_MLS_VAULT_ENABLED=false 可关闭。
  mlsVaultEnabled: (import.meta.env.VITE_MLS_VAULT_ENABLED ?? "true") === "true",
  // EN: Track A — PIN-wrapped MLS signing-key backup for offline primary recovery (design §5.3 path C).
  // DEFAULT OFF everywhere; opt in with VITE_MLS_SIGNING_PIN_BACKUP=true. Requires mlsVaultEnabled +
  // ipfs + vault_master. Use `signingPinBackupActive()` for UI/runtime gates.
  // CN: 路线 A —— PIN 包裹 MLS 签名钥备份（设计 §5.3 路径 C）。**默认关闭**；仅当
  // VITE_MLS_SIGNING_PIN_BACKUP=true 时启用。依赖 mlsVaultEnabled + ipfs + vault_master。UI/运行时用
  // `signingPinBackupActive()` 门控。
  mlsSigningPinBackupEnabled:
    (import.meta.env.VITE_MLS_SIGNING_PIN_BACKUP ?? "false") === "true",
  // EN: 1:1 Wire multi-leaf (HYBRID_DESIGN §4 / CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC). When on,
  // direct (1:1) chats run on a SEPARATE OpenMLS engine whose leaf credential is device-distinct
  // (`{account}#{device}`), enabling per-device add/remove (multi-device 1:1) and Gate-1/2 commit
  // serialization. Default OFF: the group (Track A) engine is untouched, and the wire engine uses
  // relay-only KeyPackage exchange (no chain-KP bootstrap → no cross-engine KP mismatch). Existing
  // 1:1 sessions are unaffected while off.
  // CN: 1:1 Wire 多 leaf（设计 §4 / 串行化规范）。开启后私聊（1:1）跑在**独立** OpenMLS 引擎上，其 leaf
  // 凭证按设备区分（`{account}#{device}`），支持按设备增删（多设备 1:1）与闸一/闸二 commit 串行化。默认
  // 关闭：群（轨 A）引擎不动，wire 引擎仅经 relay 交换 KeyPackage（无链上 KP 引导 → 无跨引擎 KP 失配）。
  // 关闭时既有 1:1 会话不受影响。
  wireMultileafEnabled: (import.meta.env.VITE_WIRE_MULTILEAF_ENABLED ?? "false") === "true",
  // EN: Group Wire-ification (CHAT_GROUP_WIREIFY_DESIGN §17.1). When on, the GROUP engine runs the
  // same per-device-leaf model as 1:1 Wire: a device-distinct leaf credential (`{account}#{device}`)
  // + E2EI leaf binding, holding its OWN signer — NOT the Track A read-only escrow. This removes the
  // primary/PIN/handoff machinery for groups and gives multi-device concurrent send + per-device PCS.
  // Default OFF: groups stay on Track A (read-only escrow) and behaviour is byte-identical. Implies a
  // separate `gwire:{account}` snapshot, so turning it on starts fresh Wire group state (the device
  // re-joins its groups via §6.3); migration is forward-only (§17.2).
  // CN: 群 Wire 化（设计 §17.1）。开启后**群**引擎跑与 1:1 Wire 相同的每设备 leaf 模型：设备区分 leaf 凭证
  // （`{account}#{device}`）+ E2EI leaf 绑定，持**自己的** signer——而非轨 A 只读托管。由此移除群的
  // primary/PIN/handoff 机器，获得多端并发发送 + 按设备 PCS。默认关闭：群仍走轨 A（只读托管），行为逐字节
  // 一致。开启意味着独立的 `gwire:{account}` 快照，故开启即从全新 Wire 群态起步（设备经 §6.3 重新加入其群）；
  // 迁移仅向前（§17.2）。
  wireGroupMultileafEnabled:
    (import.meta.env.VITE_WIRE_GROUP_MULTILEAF_ENABLED ?? "false") === "true",
  // EN: EISA on-chain encrypted sync anchor tier (ADR CHAT_SYNC_ANCHOR §7/§11.3):
  // "standard" publishes the anchor on the long debounce; "relay_only" never touches
  // the chain (no anchor activity metadata on-chain).
  // CN: EISA 链上加密同步锚档位（ADR CHAT_SYNC_ANCHOR §7/§11.3）："standard" 按长
  // debounce 发布锚；"relay_only" 完全不上链（链上无锚活跃度元数据）。
  syncAnchorTier: (import.meta.env.VITE_SYNC_ANCHOR_DEFAULT ?? "standard") as
    | "standard"
    | "relay_only",
  // EN: client-side long debounce for publish_sync_anchor (§11.2; chain hard cap is
  // MinBlocksBetweenPublish — the stricter one wins). Dev default 15min.
  // CN: publish_sync_anchor 的客户端长 debounce（§11.2；链上硬顶为
  // MinBlocksBetweenPublish——取较严者）。开发默认 15 分钟。
  syncAnchorDebounceMs: Number(import.meta.env.VITE_SYNC_ANCHOR_DEBOUNCE_MS ?? String(15 * 60_000)),
  // EN: EISA extrinsic payer (ADR §11.1, P3): "main" = primary account (fee link
  // disclosed, §5.7); "burner" = dedicated payer derived from vault_master — removes the
  // main account's per-publish trail (one-time funding transfer stays visible).
  // CN: EISA extrinsic 付费方（ADR §11.1，P3）："main" = 主账户（付费关联如实披露，
  // §5.7）；"burner" = 由 vault_master 派生的专用付费账户——消除主账户逐次 publish 痕迹
  // （一次性充值转账仍可见）。
  syncAnchorPayer: (import.meta.env.VITE_SYNC_ANCHOR_PAYER ?? "main") as "main" | "burner",
  // EN: RFC 9474 blind delivery tokens for 1:1 (requires relay inbox_register path).
  // CN: 1:1 的 RFC 9474 盲签投递令牌（需 relay inbox_register）。
  deliveryTokensEnabled: (() => {
    const relay = (import.meta.env.VITE_RELAY_WS ?? "").length > 0;
    if (!relay) return false;
    const env = import.meta.env.VITE_DELIVERY_TOKENS_ENABLED;
    if (env != null && env !== "") return env === "true";
    return (import.meta.env.VITE_USE_MOCK ?? "true") !== "true";
  })(),
  deliveryModulusBits: Number(import.meta.env.VITE_DELIVERY_RSA_MODULUS ?? "3072"),
  // EN: blind tokens per token_req batch (smaller = faster first message; refill on open chat).
  // CN: 每次 token_req 批量盲签张数（越小首条越快；打开会话时会预取）。
  deliveryTokenBatch: Number(import.meta.env.VITE_DELIVERY_TOKEN_BATCH ?? "8"),
  // EN: Show the welcome-screen //Seed dev shortcut (local multi-tab chain tests). When false,
  // production still uses the built-in desktop wallet via WalletGate — NOT a browser extension.
  // CN: 欢迎页 //Seed dev 快捷入口（本地多标签链上联调）。为 false 时生产仍走 WalletGate 内置
  // 桌面钱包——并非浏览器扩展钱包。
  devWallet: (import.meta.env.VITE_DEV_WALLET ?? "true") === "true",
  // EN: Optional NEXCOM DApp URL for full market trading (discover → 市场).
  // CN: 可选 NEXCOM DApp 外链，用于发现页「市场」完整交易。
  marketDappUrl: import.meta.env.VITE_MARKET_DAPP_URL ?? "",
  // EN: Optional NEXCOM DApp URL for product purchase (discover → 购物).
  // CN: 可选 NEXCOM DApp 外链，用于发现页「购物」商品购买。
  shopDappUrl: import.meta.env.VITE_SHOP_DAPP_URL ?? "",
  // EN: Default entity id when user has not chosen one (e.g. VITE_DEFAULT_ENTITY_ID=100010).
  // CN: 用户未手动选择时的默认 Entity id（如 VITE_DEFAULT_ENTITY_ID=100010）。
  defaultEntityId: (() => {
    const raw = import.meta.env.VITE_DEFAULT_ENTITY_ID;
    if (raw == null || String(raw).trim() === "") return null;
    const n = Number(raw);
    return Number.isFinite(n) && n > 0 ? n : null;
  })(),
  // EN: Decentralized 1:1 Double Ratchet stack (CHAT_1TO1_X3DH_DOUBLE_RATCHET_DESIGN §11/§13).
  // When on, unlock assembles the DR engine + MultiDeviceRouter + 2↔3 ChatOrchestrator
  // (`assembleChatStack`) so 1:1 chats negotiate DR first (§20), with MLS-Wire fallback.
  // Default ON when a relay is configured (`VITE_DR_ENABLED=false` to opt out). No relay → off.
  // CN: 去中心化 1:1 双棘轮栈（设计 §11/§13）。开启后解锁装配 DR 引擎 + MultiDeviceRouter + 2↔3
  // ChatOrchestrator（`assembleChatStack`），1:1 优先协商 DR（§20），回退 MLS-Wire。已配置 relay 时
  // 默认开启（`VITE_DR_ENABLED=false` 可显式关闭）；无 relay 则关闭。
  drEnabled: (() => {
    const relay = (import.meta.env.VITE_RELAY_WS ?? "").length > 0;
    if (!relay) return false;
    const env = import.meta.env.VITE_DR_ENABLED;
    if (env != null && env !== "") return env === "true";
    return true;
  })(),
  // EN: Delete undecryptable (stale-cipher) mailbox frames via `chat_consume`. Default OFF —
  // multi-device accounts would lose mail for offline siblings. Rely on TTL + client dedup.
  // CN: 经 `chat_consume` 删除无法解密的陈旧邮箱帧。默认关闭——多设备账户会使离线兄弟设备丢信；
  // 依赖 TTL + 客户端去重。
  chatMailboxConsumeStaleEnabled:
    (import.meta.env.VITE_CHAT_MAILBOX_CONSUME_STALE ?? "false") === "true",
} as const;

/// EN: True when PIN signing-key backup is fully active (env flag + vault + IPFS + non-mock).
/// CN: PIN 签名钥备份是否完全启用（环境开关 + vault + IPFS + 非 mock）。
export function signingPinBackupActive(): boolean {
  return (
    config.mlsSigningPinBackupEnabled &&
    config.mlsVaultEnabled &&
    config.ipfsEnabled &&
    !config.useMock
  );
}

/// EN: Production builds MUST NOT ship with mock mode (wrong KeyVault root + BC relay + chain bypass).
/// CN: 生产构建**禁止**以 mock 模式发布（错误的 KeyVault 根 + BC relay + 绕过链）。
if (import.meta.env.PROD && config.useMock) {
  throw new Error(
    "[nexchat] FATAL: VITE_USE_MOCK=true in a production build. Set VITE_USE_MOCK=false in .env.production before vite build --mode production.",
  );
}
