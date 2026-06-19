# NexChat

E2EE (MLS / RFC 9420) IM web client for the Nexus `pallets/chat/*` subsystem.
Implements the architecture in [`../pallets/chat/CHAT_FRONTEND_PLAN.md`](../pallets/chat/CHAT_FRONTEND_PLAN.md).

> 链只是 DS/AS + System 通知 + 投递信箱 + 权限的薄层；人类消息全部链下（MLS + relay + IPFS）。
> 首页**不是** `chat_listConversations`，而是「链上切片 ⊕ 链下状态」的 Merge 结果。

## 快速开始 / Quick start

```bash
cp .env.example .env      # 默认 VITE_USE_MOCK=true，无需节点即可跑
npm install
npm run dev               # http://localhost:5173
npm test                  # Merge 宪法测试 T1–T10
npm run typecheck
```

### Android App（Capacitor）

薄壳 + 远程 CDN 加载，前端可热更新。详见 [`docs/ANDROID.md`](docs/ANDROID.md)。

```bash
export CAP_SERVER_URL=https://你的前端域名   # 或局域网 http://IP:5173 联调
npm run build:cap && CAP_SERVER_URL=$CAP_SERVER_URL npx cap sync android
npm run cap:open    # Android Studio 运行 / 打 APK
```

设为 `VITE_USE_MOCK=false` 后，读走 node 的 `chat_*` JSON-RPC（`VITE_HTTP_ENDPOINT`），
写走 polkadot.js + **内置桌面钱包**（`WalletGate` 创建/导入/解锁，见 [`docs/WALLET.md`](docs/WALLET.md)）；
不再使用浏览器扩展钱包。

### 链上联调 / Live-chain smoke test

```bash
./target/release/nexus-node --dev          # 仓库根，先起 dev 节点
node scripts/chain-smoke.mjs                # 跑通真实链上 MLS 群生命周期并播种数据
npm run dev                                 # VITE_USE_MOCK=false 时 UI 读取链上真实群
```

`scripts/chain-smoke.mjs` 用 dev 账户（//Alice 建群、//Bob //Charlie 入群）跑通 §7 完整时序：
`publish_key_package → create_group → set_group_profile → commit(add 2, 1→3) → pending_welcome(先读)
→ claim_welcome(后删) → chat_groupMlsSnapshot / chat_listConversations 回读`，验证 ChainClient 的
RPC/extrinsic 形状与真实 runtime 一致（链只存不透明 MLS 字节并强制 welcome/delta 双射等不变量）。

## 架构 / Architecture

```
UI (React/TS, 无密钥)
  └─ ConversationStore (state/appStore.ts)        ← 进程内门面（对应方案 §3.1.2 command 集）
       ├─ ChainClient   (chain/chainClient.ts)     chat_* JSON-RPC 读 + polkadot.js 写 + mock
       ├─ MlsEngine                                 密码学唯一所在，两套实现：
       │    ├─ OpenMlsEngine (mls/openMlsEngine.ts) ★ 真实 OpenMLS(RFC 9420) WASM（mls-wasm/）
       │    └─ WebCryptoMlsEngine (mls/mlsEngine.ts) AES-GCM 占位（双标签页 demo 用）
       ├─ RelayClient   (relay/relayClient.ts)      链下投递（BroadcastChannel；Phase 4 盲签令牌）
       ├─ LocalStore    (store/localStore.ts)       本地会话/时间线（mock 内存桩 / 真实端加密 IndexedDB）
       └─ Merge 引擎    (merge/spec.ts)             纯函数：链上切片 ⊕ 链下状态（前端心脏）
```

### OpenMLS WASM 引擎 / OpenMLS engine (`mls-wasm/`)

真实 MLS 密码学由 `mls-wasm/`（Rust crate，`openmls 0.8` + RustCrypto，编译为 WASM）提供，
是**唯一**做密码学的地方。重建产物（已生成于 `src/mls-pkg/`）：

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.122
npm run mls:build      # cargo build --target wasm32 + wasm-bindgen → src/mls-pkg/
```

### Double Ratchet WASM 引擎 / DR engine (`dr-wasm/`)

1:1 私聊的 X3DH + Double Ratchet 由 `dr-wasm/`（Rust crate `nexchat-dr`，vodozemac 0.10）提供，
与 `mls-wasm` **严格解耦**。重建产物（已生成于 `src/dr-pkg/`）：

```bash
npm run dr:build       # cargo build --target wasm32 + wasm-bindgen → src/dr-pkg/
npm run test:dr        # native integration tests (dr-wasm/tests/)
npm test -- src/crypto-dr/   # TS engine + WASM end-to-end
```

设计文档：`pallets/chat/CHAT_1TO1_X3DH_DOUBLE_RATCHET_DESIGN.md`。TS 封装：`src/crypto-dr/vodozemacEngine.ts`。

`MlsClient` 暴露：`generateKeyPackage` / `createGroup` / `addMembers`(commit+welcome) /
`processWelcome` / `processCommit` / `encrypt` / `decrypt`，cipher suite =
`MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`（IANA 1）。`OpenMlsEngine` 在 TS 侧封装它，
P3 信封编解码留在 TS，字节按会话 id 路由进 WASM。

### 握手控制面 / Handshake control-plane

多标签页/多端要进入**同一个** OpenMLS 群才能互发真实 MLS 消息。控制面有两种实现，由
`VITE_MLS_CONTROL_PLANE` 选择：

> **故意双轨、勿盲目合并**：Track A 群托管 vs Wire 多 leaf、registry 握手 vs graft、1:1/群 Wire 对称模块等——见 [`docs/INTENTIONAL_DUAL_PATHS.md`](docs/INTENTIONAL_DUAL_PATHS.md)。

> **删除 / 撤回**：人类消息走链下（本地删除 + archive 墓碑、MLS `recall` 控制信封）；链上 `delete_message` / `recall_message` 仅 System 通知——见 [`docs/DELETE_AND_RECALL.md`](docs/DELETE_AND_RECALL.md)。

> **钱包**：生产使用内置软件钱包（助记词 + 密码），非浏览器扩展——见 [`docs/WALLET.md`](docs/WALLET.md)。

**`chain`（默认，真实链上 DS/AS，`mls/chainHandshake.ts`）**：直接用 chat-group pallet 当
DS/AS——链负责身份（`publish_key_package`）、成员/epoch（`commit`）、Welcome 信箱
（`pending_welcome` / `claim_welcome`）的全局排序；密码学全在 OpenMLS，链只搬运不透明字节。
名册（`VITE_MLS_ROSTER`，roster[0] 为 owner）驱动、owner 主导的轮询编排：

1. **owner**：给其他成员转账（dev 创世只给创建者发币）→ 用链将铸造的 `nextGroupId` 作 OpenMLS 键
   建群 → `create_group` 上链 → 轮询各成员链上 KeyPackage → `add_members` + `commit`（首个 commit
   须加 ≥2 人，链禁止恰好 2 人群）。
2. **成员**：等被转账 → 吊销残留 prekey 后发布一个全新 KeyPackage → 轮询会话列表发现被加入的群 →
   `pending_welcome` 取回并处理 Welcome → `claim_welcome` → 从握手日志 `handshake_at_epoch` 补齐
   后续 epoch。

**生产 / LIVE（`VITE_USE_MOCK=false`）**：用户在 `WalletGate` 创建或导入助记词账户，密码解锁后
由内置 desktop keyring（SS58 273）签名链上 extrinsic；`vault_master` 派生自已解锁 pair，驱动
KeyVault 与跨设备 blob。详见 [`docs/WALLET.md`](docs/WALLET.md)。

**本地 dev 快捷路径（可选）**：`VITE_DEV_WALLET=true` 时欢迎页显示 `//Alice` 等 dev seed 按钮，
便于多标签页链上联调；生产构建应设 `VITE_DEV_WALLET=false`（仅隐藏 dev 按钮，**不**切换为扩展钱包）。
实时节点 e2e：`npm run e2e:chain`。

**`relay`（mock/离线模拟，`mls/handshake.ts`）**：经 BroadcastChannel 控制通道
（`sendControl`/`onControl`）最小化复刻 DS/AS：hello 选举 owner（endpoint id 最小、粘滞）→ kp →
owner 惰性建群 + 定向 Welcome → 广播 Commit 推进 epoch。

`appStore` 按会话选引擎：demo 群握手完成后走真 OpenMLS，其余会话回退 WebCrypto。握手未完成时该群
禁发，UI 顶部显示 `🔒 OpenMLS · 角色 · N 端` / `⏳ 握手中`。

### OpenMLS 状态持久化 / Persistence (`mls/mlsStore.ts`)

OpenMLS 的全部密码状态（签名私钥、群、棘轮）都在 WASM provider 的存储里。`MlsClient.exportState()`
把整库快照成不透明 blob，`MlsClient.restore(blob)` 重建出**完全一致**的客户端（恢复 KV → 读回签名
密钥 → `MlsGroup::load` 每个群）。TS 侧 `OpenMlsEngine` 在每次状态变更后防抖写入 IndexedDB（以账户
地址为键），并在 `pagehide`/隐藏时立即落盘；`init` 时若有快照则 `restore`。于是页面刷新或第二设备
都能在**同一棘轮**上继续收发（无 IndexedDB 的 Node 环境自动降级为纯内存）。

### IPFS（本地 kubo）/ `ipfs/ipfsClient.ts`

需本地 **kubo** 节点（API `127.0.0.1:5001`，网关 `127.0.0.1:8080`）。`npm run dev` 时 Vite 把
`/ipfs-api` → `:5001/api/v0`、`/ipfs-gateway` → `:8080`，浏览器无需单独配 CORS。

```bash
# 安装并启动（示例）
ipfs daemon   # 默认 API :5001，网关 :8080
```

- **群头像**：群主/管理员点 🖼 → 图片 `ipfs add` → `chatGroup.setGroupProfile(avatar_cid)`；列表/聊天窗
  经网关 `/ipfs/{cid}` 展示（链上只存 CID 字符串）。
- **聊天附件**：📎 选文件 → 客户端 `AES-256-GCM` 加密 → `ipfs add` 密文 → MLS 信封带
  `root_cid + file_key`（relay/链/IPFS 均不见明文）。**>16MB 或视频**走 1MiB 分块 + 加密 manifest；
  图片/视频自动生成 ≤320px 加密缩略图（`thumb_cid`），列表/气泡**缩略图先行**，全量按需加载。

`.env`：`VITE_IPFS_ENABLED=true`（默认）、`VITE_IPFS_API_URL=/ipfs-api`、`VITE_IPFS_GATEWAY_URL=/ipfs-gateway`。

## 已落地（Phase 0 + Phase 1）

- ✅ Merge 引擎纯函数 + **T1–T10 宪法测试**（最易踩坑点全覆盖：纯链下私聊出现、同对端合并、
  群 `last_active=0` 不沉底、`muted` 按 `kind` 拆 `dnd`/`adminMuted`、角标≠`total_direct_unread`…）
- ✅ ChainClient（`chat_*` 只读 JSON-RPC + 通用签名 extrinsic + mock 回退）
- ✅ 脱敏视图模型（`ConversationVM` / `MessageVM` / `AccountVM`）
- ✅ 会话列表 + 聊天窗 UI：`dnd`(🔕) 与 `adminMuted`(🚫) 不同图标；冻结群(❄️)只读、禁言禁发
- ✅ **KeyVault**：WebCrypto HKDF 派生每会话密钥
- ✅ **P3 MLS payload 信封**编解码（`reply_to` / `mentions` / `reaction` / `ephemeral`）+ 测试
- ✅ **真实 AES-256-GCM 消息加密**（`WebCryptoMlsEngine`）—— relay 只见密文（≠ 明文断言已测）
- ✅ **BroadcastChannel relay**：同源多标签页**真实加密投递**（开两个标签页即两个用户对聊）
- ✅ 发送/接收管线：乐观发送 + 状态机（pending→sent）、入站解密→按 `convId` 路由→实时刷新→重 Merge
- ✅ UI：引用回复（↩）、阅后即焚（⏱）、实时入站
- ✅ **16 个测试全过**（merge 10 + mls 4 + 集成 2）
- ✅ **链上联调通过**：`scripts/chain-smoke.mjs` 在 dev 节点上跑通真实 MLS 群 DS/AS 生命周期
  （建群 / 入群 commit 1→3 / Welcome 先读后删），UI 在 `VITE_USE_MOCK=false` 下读取并渲染链上真实群
- ✅ **真实 OpenMLS(RFC 9420) WASM 引擎接入**（`mls-wasm/` → `src/mls-pkg/`）：
  - 引擎单测：3 个独立客户端真实握手（建群→加 2 人→Welcome→process）+ 应用消息收发 + 非成员解密失败（19 测试全过）
  - 浏览器验证：Vite `?url` 加载 wasm，浏览器内跑通握手 + P3 信封 AEAD round-trip
  - **真字节链上联调**：`scripts/chain-smoke.mjs` 现用**真实** KeyPackage / Commit / Welcome /
    tree·transcript hash 提交链上 `publish_key_package` / `create_group` / `commit` 并被接受，
    新成员从链上取回 Welcome 经 OpenMLS 入群，最后完成 Alice→Bob/Charlie 的 E2EE 应用消息解密
- ✅ **握手控制面（relay 模拟）接入运行中 App**：多标签页经 relay 完成 owner 选举 / KeyPackage /
  commit·welcome / epoch 补齐；握手单测（2 标签页选举+收发、3 标签页 epoch 补齐）通过
- ✅ **真实链上 DS/AS 控制面**（`mls/chainHandshake.ts`）：用链上 `publish_key_package` / `create_group`
  / `commit` / `pending_welcome` / `claim_welcome` / `handshake_at_epoch` 当控制面，relay 仅承载应用
  消息密文。ChainClient 增 dev keyring 签名（SS58 42）+ 链上 KeyPackage / nextGroupId / 余额读取。
  **实时节点 e2e 通过**（`npm run e2e:chain`，33s）：fresh owner 充值→建群→commit 加 2 人；成员从零
  被充值→发布 KeyPackage→发现群→领取 Welcome；真实 OpenMLS 应用消息 owner→2 成员往返
- ✅ **OpenMLS 状态持久化**（`exportState`/`restore` + IndexedDB）：成员/owner 经 export→restore 在
  同一棘轮上继续收发；owner restore 后仍能 commit 新成员
- ✅ **聊天记录加密落库**（`store/encryptedLocalStore.ts`）：真实浏览器会话把消息时间线 + 本地会话
  偏好以 **AES-256-GCM** 静态加密写入 IndexedDB（密钥由 `KeyVault` 按账户派生、不可导出，跨刷新确定
  可解密），跨刷新/重开保留消息；mock 与 Node/测试环境自动降级为内存
- ✅ **最小 IPFS 客户端**（`ipfs/ipfsClient.ts`）：经本地 **kubo**（`:5001` API + `:8080` 网关）上传/
  按 CID 抓取。Vite dev 用 `/ipfs-api`、`/ipfs-gateway` 代理规避 CORS。群头像明文上传 CID 写链
  `set_group_profile`；聊天附件**先 AES-GCM 加密再 add**，`file_key` 仅在 MLS 信封内 E2EE 下发。
  UI：📎 发附件、🖼 群主/管理员换群头像、会话列表/聊天窗按 CID 渲染头像、图片预览/文件下载；
  **分块+manifest**（>16MB/视频）、**缩略图先行**（≤320px 加密 thumb，全量按需拉取）
- ✅ **阅后即焚执行**（`ephemeral/`）：`burn_on: read|deliver` 语义、打开会话启动 read 倒计时、
  1s 本地清理循环（加密 IndexedDB 同步删除）、relay `expiresAt` 过期丢弃、气泡倒计时 UI
- ✅ **可选链上 Pin**（`ipfs/pin.ts`，`VITE_IPFS_PIN_ENABLED`，**默认关**）：非 ephemeral 群聊附件
  上传后可 opt-in `storageService.requestPinForSubject`（thumb Standard / 正文 Temporary）
- ✅ **发送方本机媒体 retention**（`ipfs/senderMediaRetention.ts` + `VITE_IPFS_MEDIA_LOCAL_PIN`）：
  聊天媒体默认**不做链上/运营全局 pin**；本机 kubo pin + TTL（默认 30 天）+ 每小时清扫到期
  尽力 unpin；阅后即焚 `pin=false` 且不登记；sync blob 仍始终本机 pin
- ✅ **媒体下载确认提前释放**（`media_ack` MLS 控制信封）：1:1 接收方全量下载成功后回发
  `media_ack`，发送方将该消息的本机 pin TTL 收短为 1h 宽限（群聊不做全员跟踪，走完整 TTL）
- ✅ **保留附件 ☆**（气泡按钮 → `keepAttachment`）：标星豁免本地清理；发送方移出 retention
  登记（本机 pin 长期保留）；LIVE 模式追加链上 Pin（正文 Temporary / 缩略图 Standard）
- ✅ **P3 转发 + @提及**（`p3/`）：信封 `forward` / `mentions` 字段；气泡转发卡片 + ➡ 按钮；
  群聊 `@Alice` 解析为 roster 成员引用；本地 `mentionUnread` 索引 + 会话列表 `@` 角标
- ✅ **P3 reaction 表情**（`p3/reactions.ts`）：轻量 `type=reaction` MLS 消息；客户端按 `target`
  聚合 add/remove；气泡下表情芯片 + 快捷选择器（👍❤️😂🎉😮）；reaction 不计入未读
- ✅ **网络化 relay（可选 WS）**（`relay/wsRelay.ts` + Rust `relay-rs`）：`VITE_RELAY_WS`
  配置后与 BroadcastChannel 多路复用（`MultiplexRelay`），跨机器/浏览器 profile 可互通；入站按
  `dedupKey` 去重。启动：`npm run relay:server`（构建并运行 `relay-rs`，默认 `ws://127.0.0.1:8765`）。
- ✅ **Pin 续费 UI**（`ui/PinPanel.tsx` + `chain/pinQueries.ts`）：LIVE 模式顶栏 📌 打开面板，读取
  `storageService.ownerPinIndex` / `pinMeta` / `cidRegistry` / `billingQueue` 宽限期状态，支持
  `renew_pin` 预付 1 周期。

> 说明：`WebCryptoMlsEngine` 仅作非 demo 会话的传输加密占位（会话密钥由共享种子 + `convId` 派生）。
> demo 群已全程走**真实 OpenMLS**。架构不变量成立：**relay/链始终只见密文**，密码学只在 `MlsEngine`，
> 密钥只在 OpenMLS provider 存储 / `KeyVault`。

### 跨机器联调 / Cross-machine relay

```bash
npm run relay:server              # 终端 1：Rust relay-rs WS :8765（含 conv-index 指针 KV）
./target/release/nexus-node --dev # 终端 2：链
ipfs daemon                       # 终端 3：IPFS（附件/头像）
# .env: VITE_USE_MOCK=false, VITE_RELAY_WS=ws://127.0.0.1:8765
npm run dev                       # 各端浏览器 ?as=//Alice|//Bob|//Charlie
```

## 待接入（后续 Phase）

- ✅ **1:1 私聊 MVP**（`mls/directHandshake.ts`）：链下成对 OpenMLS（**不建链上群**），规范 MLS 键
  `d:{sorted_a}:{sorted_b}`、UI 路由 `d:{peer}`；owner = 字典序较小地址；KeyPackage 来自链上或
  relay kp；顶栏名册「＋ 私聊」发起。
- ✅ **RFC 9474 盲签投递令牌**（`delivery/` + `@cloudflare/blindrsa-ts`）：接收方 inbox（IPK
  + epoch）注册 relay；发送方盲化 `H(t‖ct‖epoch)` 批量申领；1:1 发送附带 `delivery` 准入 +
  sealed-sender；relay 离线验签 + per-inbox `SpentSet`（Rust `relay-rs` `server/src/token.rs`）。
  `VITE_DELIVERY_TOKENS_ENABLED=true`（LIVE 默认开；MOCK 默认关）。
- ✅ **跨设备 conv-index**（`store/convIndex.ts` + `convIndexSync.ts`）：本地会话/偏好快照 →
  `KDF(account,"chat/conv-index/v1")` AES-GCM 加密 → IPFS `add`；指针经 relay `index_put` /
  `index_fetch`（Rust `relay-rs`）+ localStorage 兜底；解锁时拉取合并（LWW）。
  需 `VITE_CONV_INDEX_ENABLED=true` + IPFS +（跨机）`VITE_RELAY_WS`。
- ✅ **联系人添加通知**（`contacts/contactRequestExchange.ts` + `relay/contactRequestInbox.ts`）：
  添加联系人时经 relay 控制面发送 `contact_req`；relay 按 `toAddr` 邮箱持久化（30 天 TTL），对端
  **离线时**在下次解锁经 `contact_fetch` 补发；在线则实时投递。UI「联系人」Tab 可接受/拒绝
  （`contact_ack`，同样持久化）；接受后双方通讯录互相同步（localStorage，不上链）。
  需 `VITE_RELAY_WS`（或同源 BC 仅实时、不跨重启补发）。
- 私群入群走 `request_join` / `approve_join`（当前 demo 用公开群免审批 Add）
- 把静态加密密钥从演示主种子换成解锁的账户主密钥（安全存储），并可选 SQLCipher-wasm 提供更强查询/索引
- 链上 `register_inbox` / `chat_inboxEpoch` 与 relay 校验联动（当前 relay 信任客户端注册）
- Pin 宽限期链上事件订阅 / 推送告警（当前为手动刷新面板）
